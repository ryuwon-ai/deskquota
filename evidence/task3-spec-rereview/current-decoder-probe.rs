#![allow(dead_code)]
const MAX_EVENT_METADATA: usize=256*1024; const EVENT_NAME_BYTES:usize=32;const BOM_BYTES:usize=3;const EVENT_STORAGE_BYTES:usize=MAX_EVENT_METADATA-EVENT_NAME_BYTES-BOM_BYTES;
#[derive(Clone,Copy)] struct Endpoint;
struct EndpointObserver { events:Vec<(Option<String>,Vec<u8>)> }
impl EndpointObserver {fn new(_:Endpoint)->Self{Self{events:vec![]}}fn observe(&mut self,e:Option<&str>,d:&[u8]){self.events.push((e.map(str::to_owned),d.to_vec()));}fn mark_invalid(&mut self){} }
struct SseDecoder {
    endpoint: EndpointObserver,
    storage: Box<[u8]>,
    used: usize,
    line_start: usize,
    line_kind: LineKind,
    event: SseEvent,
    event_name: [u8; EVENT_NAME_BYTES],
    event_name_len: usize,
    event_name_overflow: bool,
    has_data: bool,
    pending_cr: bool,
    bom_checked: bool,
    bom_prefix: [u8; BOM_BYTES],
    bom_prefix_len: usize,
    overflowed: bool,
}

#[derive(Clone, Copy)]
enum LineKind {
    Field,
    Data { strip_space: bool },
    Event { strip_space: bool },
    Ignore,
}

#[derive(Clone, Copy)]
enum SseEvent {
    Default,
    MessageStart,
    MessageDelta,
    ContentBlockDelta,
    MessageStop,
    Unknown,
}

impl SseEvent {
    fn from_bytes(bytes: &[u8], overflowed: bool) -> Self {
        if overflowed {
            return Self::Unknown;
        }
        match bytes {
            b"" => Self::Default,
            b"message_start" => Self::MessageStart,
            b"message_delta" => Self::MessageDelta,
            b"content_block_delta" => Self::ContentBlockDelta,
            b"message_stop" => Self::MessageStop,
            _ => Self::Unknown,
        }
    }

    fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::MessageStart => Some("message_start"),
            Self::MessageDelta => Some("message_delta"),
            Self::ContentBlockDelta => Some("content_block_delta"),
            Self::MessageStop => Some("message_stop"),
            Self::Unknown => Some("unknown"),
        }
    }
}

impl SseDecoder {
    fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint: EndpointObserver::new(endpoint),
            storage: vec![0; EVENT_STORAGE_BYTES].into_boxed_slice(),
            used: 0,
            line_start: 0,
            line_kind: LineKind::Field,
            event: SseEvent::Default,
            event_name: [0; EVENT_NAME_BYTES],
            event_name_len: 0,
            event_name_overflow: false,
            has_data: false,
            pending_cr: false,
            bom_checked: false,
            bom_prefix: [0; BOM_BYTES],
            bom_prefix_len: 0,
            overflowed: false,
        }
    }

    fn observe(&mut self, mut bytes: &[u8]) {
        if self.overflowed {
            return;
        }
        if !self.bom_checked {
            let mut consumed = 0;
            while consumed < bytes.len() && self.bom_prefix_len < BOM_BYTES {
                self.bom_prefix[self.bom_prefix_len] = bytes[consumed];
                self.bom_prefix_len += 1;
                consumed += 1;
                let bom = b"\xef\xbb\xbf";
                if !bom.starts_with(&self.bom_prefix[..self.bom_prefix_len]) {
                    self.bom_checked = true;
                    let prefix = self.bom_prefix;
                    let prefix_len = self.bom_prefix_len;
                    self.bom_prefix_len = 0;
                    self.process(&prefix[..prefix_len]);
                    break;
                }
                if self.bom_prefix_len == BOM_BYTES {
                    self.bom_checked = true;
                    self.bom_prefix_len = 0;
                    break;
                }
            }
            bytes = &bytes[consumed..];
        }
        self.process(bytes);
    }

    fn process(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.overflowed {
                return;
            }
            if self.pending_cr {
                self.pending_cr = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => {
                    self.finish_line();
                    self.pending_cr = true;
                }
                b'\n' => self.finish_line(),
                _ => self.process_line_byte(byte),
            }
        }
    }

    fn process_line_byte(&mut self, byte: u8) {
        match self.line_kind {
            LineKind::Field if self.used == self.line_start && byte == b':' => {
                self.line_kind = LineKind::Ignore;
            }
            LineKind::Field if byte == b':' => {
                let is_data = &self.storage[self.line_start..self.used] == b"data";
                let is_event = &self.storage[self.line_start..self.used] == b"event";
                self.used = self.line_start;
                if is_data {
                    if self.has_data {
                        self.push_storage(b'\n');
                    }
                    self.line_kind = LineKind::Data { strip_space: true };
                } else if is_event {
                    self.reset_event_name();
                    self.line_kind = LineKind::Event { strip_space: true };
                } else {
                    self.line_kind = LineKind::Ignore;
                }
            }
            LineKind::Field => self.push_storage(byte),
            LineKind::Data { strip_space: true } if byte == b' ' => {
                self.line_kind = LineKind::Data { strip_space: false };
            }
            LineKind::Data { .. } => {
                self.line_kind = LineKind::Data { strip_space: false };
                self.push_storage(byte);
            }
            LineKind::Event { strip_space: true } if byte == b' ' => {
                self.line_kind = LineKind::Event { strip_space: false };
            }
            LineKind::Event { .. } => {
                self.line_kind = LineKind::Event { strip_space: false };
                if self.event_name_len < EVENT_NAME_BYTES {
                    self.event_name[self.event_name_len] = byte;
                    self.event_name_len += 1;
                } else {
                    self.event_name_overflow = true;
                }
            }
            LineKind::Ignore => {}
        }
    }

    fn finish_line(&mut self) {
        match self.line_kind {
            LineKind::Field if self.used == self.line_start => self.dispatch_event(),
            LineKind::Field => {
                let is_data = &self.storage[self.line_start..self.used] == b"data";
                let is_event = &self.storage[self.line_start..self.used] == b"event";
                self.used = self.line_start;
                if is_data {
                    if self.has_data {
                        self.push_storage(b'\n');
                    }
                    self.has_data = true;
                } else if is_event {
                    self.event = SseEvent::Default;
                }
            }
            LineKind::Data { .. } => self.has_data = true,
            LineKind::Event { .. } => {
                self.event = SseEvent::from_bytes(
                    &self.event_name[..self.event_name_len],
                    self.event_name_overflow,
                );
            }
            LineKind::Ignore => {}
        }
        self.line_start = self.used;
        self.line_kind = LineKind::Field;
        self.reset_event_name();
    }

    fn dispatch_event(&mut self) {
        if self.has_data {
            self.endpoint
                .observe(self.event.as_str(), &self.storage[..self.used]);
        }
        self.used = 0;
        self.line_start = 0;
        self.has_data = false;
        self.event = SseEvent::Default;
    }

    fn push_storage(&mut self, byte: u8) {
        if self.used == self.storage.len() {
            self.mark_overflow();
            return;
        }
        self.storage[self.used] = byte;
        self.used += 1;
    }

    fn reset_event_name(&mut self) {
        self.event_name_len = 0;
        self.event_name_overflow = false;
    }

    fn mark_overflow(&mut self) {
        self.overflowed = true;
        self.endpoint.mark_invalid();
    }

    #[cfg(test)]
    fn retained_buffer_capacity(&self) -> usize {
        self.storage.len() + self.event_name.len() + self.bom_prefix.len()
    }
}


fn main(){
let mut d=SseDecoder::new(Endpoint);
d.observe(&[b"data: ".to_vec(),vec![b'x';70000],b"\ndata: y\n".to_vec(),b"event: ".to_vec(),vec![b'z';60000],b"\n:".to_vec(),vec![b'a';131071]].concat());
let retained=d.storage.len()+d.event_name.len()+d.bom_prefix.len(); assert_eq!(retained,MAX_EVENT_METADATA);assert!(!d.overflowed);println!("current decoder retained={} limit={} overflow={}",retained,MAX_EVENT_METADATA,d.overflowed);
let wire=b"\xef\xbb\xbfevent: message_delta\r\ndata: one\rdata: two\n: comment\r\nunknown: ignored\n\nevent\ndata\n\ndata: unfinished";
for chunk in 1..=wire.len(){let mut d=SseDecoder::new(Endpoint);for b in wire.chunks(chunk){d.observe(b);}assert_eq!(d.endpoint.events,vec![(Some("message_delta".into()),b"one\ntwo".to_vec()),(None,vec![])]);}
println!("current decoder framing chunk sizes 1..={} PASS; split BOM/CR/LF/CRLF/multidata/bare data/event/comments/unknown fields/incomplete EOF",wire.len());
}
