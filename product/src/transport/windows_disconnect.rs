//! Mio reports Windows TCP resets as closed readiness, not ERROR. Subscribe to
//! FD_CLOSE directly: no periodic polling, body reads, or thread per connection.
#![allow(unsafe_code)]
use std::{
    ffi::c_void,
    io,
    net::TcpStream,
    os::windows::io::{AsRawHandle, AsRawSocket, FromRawHandle, OwnedHandle},
    ptr::{null, null_mut},
};
use tokio::sync::Notify;
use windows_sys::Win32::{Networking::WinSock::*, System::Threading::*};

struct CloseWait {
    socket: TcpStream,
    event: OwnedHandle,
    wait: PTP_WAIT,
    notification: Box<Notify>,
}

unsafe extern "system" fn notify_close(
    _: PTP_CALLBACK_INSTANCE,
    context: *mut c_void,
    _: PTP_WAIT,
    _: u32,
) {
    // SAFETY: CloseWait keeps this allocation alive until all callbacks finish.
    unsafe { &*context.cast::<Notify>() }.notify_one();
}

impl CloseWait {
    fn new(socket: TcpStream) -> io::Result<Self> {
        // SAFETY: unnamed manual-reset event, initially unsignaled, no inherited handle.
        let event = unsafe { CreateEventW(null(), 1, 0, null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let event = unsafe { OwnedHandle::from_raw_handle(event) };
        let mut notification = Box::new(Notify::new());
        // SAFETY: allocation remains stable; the wait is not armed until owned below.
        let wait = unsafe {
            CreateThreadpoolWait(
                Some(notify_close),
                (&mut *notification as *mut Notify).cast(),
                null(),
            )
        };
        if wait == 0 {
            return Err(io::Error::last_os_error());
        }
        let owned = Self {
            socket,
            event,
            wait,
            notification,
        };
        // SAFETY: live socket/event, owned wait; FD_CLOSE does not consume data
        // or cancel Mio's AFD readiness registration on the other socket handle.
        unsafe {
            if WSAEventSelect(
                owned.socket.as_raw_socket() as SOCKET,
                owned.event.as_raw_handle() as WSAEVENT,
                FD_CLOSE as i32,
            ) == SOCKET_ERROR
            {
                return Err(io::Error::from_raw_os_error(WSAGetLastError()));
            }
            SetThreadpoolWait(owned.wait, owned.event.as_raw_handle(), null());
        }
        Ok(owned)
    }
}

impl Drop for CloseWait {
    fn drop(&mut self) {
        // SAFETY: disarm first, cancel queued callbacks and join the tiny running
        // callback before freeing its context or event. Never called in callback.
        unsafe {
            SetThreadpoolWait(self.wait, null_mut(), null());
            WaitForThreadpoolWaitCallbacks(self.wait, 1);
            CloseThreadpoolWait(self.wait);
            WSAEventSelect(self.socket.as_raw_socket() as SOCKET, 0, 0);
        }
    }
}

pub(crate) async fn reset(socket: TcpStream) -> io::Result<bool> {
    let owned = CloseWait::new(socket)?;
    owned.notification.notified().await;
    let mut events = WSANETWORKEVENTS::default();
    // SAFETY: live socket/event and initialized output, queried before Drop.
    unsafe {
        if WSAEnumNetworkEvents(
            owned.socket.as_raw_socket() as SOCKET,
            owned.event.as_raw_handle() as WSAEVENT,
            &mut events,
        ) == SOCKET_ERROR
        {
            return Err(io::Error::from_raw_os_error(WSAGetLastError()));
        }
    }
    // A graceful send-half-close still permits the client to receive its reply.
    Ok(events.lNetworkEvents & FD_CLOSE as i32 != 0
        && events.iErrorCode[FD_CLOSE_BIT as usize] != 0)
}
