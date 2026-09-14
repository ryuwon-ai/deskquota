#![allow(dead_code)]
const MAX_EVENT_METADATA: usize = 256*1024;
#[derive(Clone, Copy)] struct Endpoint;
struct EndpointObserver;
impl EndpointObserver { fn new(_:Endpoint)->Self {Self} fn observe(&mut self,_:Option<&str>,_:&[u8]) {} fn mark_invalid(&mut self) {} }
struct SseDecoder {
    endpoint: EndpointObserver,
    line: Vec<u8>,
    line_overflow: bool,
    data: Vec<u8>,
    event: Vec<u8>,
    has_data: bool,
    event_discarded: bool,
    pending_cr: bool,
    bom_checked: bool,
    bom_prefix: Vec<u8>,
    overflowed: bool,
}

impl SseDecoder {
    fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint: EndpointObserver::new(endpoint),
            line: Vec::new(),
            line_overflow: false,
            data: Vec::new(),
            event: Vec::new(),
            has_data: false,
            event_discarded: false,
            pending_cr: false,
            bom_checked: false,
            bom_prefix: Vec::new(),
            overflowed: false,
        }
    }

    fn observe(&mut self, mut bytes: &[u8]) {
        if self.overflowed {
            return;
        }
        if !self.bom_checked {
            let mut consumed = 0;
            while consumed < bytes.len() && self.bom_prefix.len() < 3 {
                self.bom_prefix.push(bytes[consumed]);
                consumed += 1;
                let bom = b"\xef\xbb\xbf";
                if !bom.starts_with(&self.bom_prefix) {
                    self.bom_checked = true;
                    let prefix = std::mem::take(&mut self.bom_prefix);
                    self.process(&prefix);
                    break;
                }
                if self.bom_prefix.len() == 3 {
                    self.bom_checked = true;
                    self.bom_prefix.clear();
                    break;
                }
            }
            bytes = &bytes[consumed..];
        }
        self.process(bytes);
    }

    fn process(&mut self, bytes: &[u8]) {
        for &byte in bytes {
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
                _ if self.line.len() < MAX_EVENT_METADATA / 2 => self.line.push(byte),
                _ => self.line_overflow = true,
            }
        }
    }

    fn finish_line(&mut self) {
        let line = std::mem::take(&mut self.line);
        let line_overflow = std::mem::take(&mut self.line_overflow);
        if line.is_empty() && !line_overflow {
            self.dispatch_event();
            return;
        }
        if line.first() == Some(&b':') {
            return;
        }
        let colon = line.iter().position(|byte| *byte == b':');
        let (field, mut value) = colon.map_or((&line[..], &[][..]), |index| {
            (&line[..index], &line[index + 1..])
        });
        if value.first() == Some(&b' ') {
            value = &value[1..];
        }
        match field {
            b"data" => {
                if line_overflow
                    || self.data.len() + self.event.len() + value.len() + usize::from(self.has_data)
                        > MAX_EVENT_METADATA / 2
                {
                    self.mark_overflow();
                    self.event_discarded = true;
                    return;
                }
                if self.has_data {
                    self.data.push(b'\n');
                }
                self.data.extend_from_slice(value);
                self.has_data = true;
            }
            b"event" => {
                if line_overflow || self.data.len() + value.len() > MAX_EVENT_METADATA / 2 {
                    self.mark_overflow();
                    self.event_discarded = true;
                    return;
                }
                self.event = Vec::new();
                self.event.extend_from_slice(value);
            }
            _ => {}
        }
    }

    fn dispatch_event(&mut self) {
        if self.has_data && !self.event_discarded {
            let event = if self.event.is_empty() {
                None
            } else {
                match std::str::from_utf8(&self.event) {
                    Ok(event) => Some(event),
                    Err(_) => {
                        self.endpoint.mark_invalid();
                        None
                    }
                }
            };
            self.endpoint.observe(event, &self.data);
        }
        self.data = Vec::new();
        self.event = Vec::new();
        self.has_data = false;
        self.event_discarded = false;
    }

    fn mark_overflow(&mut self) {
        self.overflowed = true;
        self.endpoint.mark_invalid();
    }
}


fn main() {
let mut s=SseDecoder::new(Endpoint);
s.observe(&[b"data: ".to_vec(),vec![b'x';70000],b"\ndata: y\n".to_vec(),b"event: ".to_vec(),vec![b'z';60000],b"\n".to_vec(),b":".to_vec(),vec![b'a';131071]].concat());
println!("line={} data={} event={} bom={} total={} limit={} overflow={}", s.line.capacity(),s.data.capacity(),s.event.capacity(),s.bom_prefix.capacity(), s.line.capacity()+s.data.capacity()+s.event.capacity()+s.bom_prefix.capacity(),MAX_EVENT_METADATA,s.overflowed);
}
