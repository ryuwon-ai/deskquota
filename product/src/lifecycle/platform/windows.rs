//! The only unsafe module: Win32 has no complete maintained safe creation +
//! handle ACL verification wrapper in our dependency set. Every returned File
//! has been checked before any caller can write token bytes. Never edit an
//! existing ACL. Runtime Windows acceptance is required beyond cross-checking.
#![allow(unsafe_code)]
use std::{
    fs::File,
    io,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    process::Command,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::Threading::*,
};
pub fn detach_session() -> io::Result<()> {
    Ok(())
}
pub fn configure_detached(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x8 | 0x200);
}
fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "state must have a current-user-only ACL, owner, and no reparse points",
    )
}
fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut s: Vec<u16> = path.as_os_str().encode_wide().collect();
    if s.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NUL in state path",
        ));
    }
    s.push(0);
    Ok(s)
}
struct User {
    buffer: Vec<usize>,
}
impl User {
    fn current() -> io::Result<Self> {
        let mut raw = null_mut();
        // SAFETY: valid pseudo process handle, one initialized out-pointer. OwnedHandle
        // takes the successfully returned token exactly once and closes on every exit.
        unsafe {
            check(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw))?;
        }
        let token = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut needed = 0;
        // SAFETY: the documented null-buffer sizing call writes only needed.
        unsafe {
            GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut needed);
        }
        if !(size_of::<TOKEN_USER>() as u32..=65536).contains(&needed) {
            return Err(denied());
        }
        let mut user = Self {
            buffer: vec![0usize; (needed as usize).div_ceil(size_of::<usize>())],
        };
        let capacity = (user.buffer.len() * size_of::<usize>()) as u32;
        // SAFETY: aligned owned allocation has capacity bytes, remains alive with the
        // TOKEN_USER and its inline SID. The API returns a bounded length we recheck.
        unsafe {
            check(GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                user.buffer.as_mut_ptr().cast(),
                capacity,
                &mut needed,
            ))?;
        }
        if needed > capacity || needed < (size_of::<TOKEN_USER>() as u32) {
            return Err(denied());
        }
        let sid = user.sid();
        let start = user.buffer.as_ptr() as usize;
        let end = start + needed as usize;
        if (sid as usize) < start || !sid_fits(sid, end) {
            return Err(denied());
        }
        Ok(user)
    }
    fn sid(&self) -> PSID {
        // SAFETY: construction initialized at least TOKEN_USER bytes in an aligned
        // allocation; the returned pointer is borrowed only while self lives.
        unsafe { (*(self.buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid }
    }
}
struct LocalString(windows_sys::core::PWSTR);
impl Drop for LocalString {
    fn drop(&mut self) {
        // SAFETY: ConvertSidToStringSidW allocates this buffer with LocalAlloc.
        unsafe {
            LocalFree(self.0.cast());
        }
    }
}
pub fn current_user_identity() -> io::Result<String> {
    let user = User::current()?;
    let mut raw = null_mut();
    // SAFETY: user owns a validated current-process token SID for this call;
    // the returned LocalAlloc buffer is immediately placed under RAII.
    unsafe {
        check(ConvertSidToStringSidW(user.sid(), &mut raw))?;
    }
    if raw.is_null() {
        return Err(denied());
    }
    let identity = LocalString(raw);
    let mut length = 0usize;
    // SAFETY: SID strings are NUL-terminated by ConvertSidToStringSidW. The
    // documented maximum SID string is far below this defensive 256-unit cap.
    unsafe {
        while length < 256 && *identity.0.add(length) != 0 {
            length += 1;
        }
        if length == 256 {
            return Err(denied());
        }
        String::from_utf16(std::slice::from_raw_parts(identity.0, length)).map_err(|_| denied())
    }
}
fn sid_fits(sid: PSID, end: usize) -> bool {
    let start = sid as usize;
    if sid.is_null() || start.checked_add(8).is_none_or(|n| n > end) {
        return false;
    }
    // SAFETY: caller supplies an OS-returned pointer inside a live allocation;
    // eight bytes fit. Check subauthority size before invoking SID validation.
    unsafe {
        let count = *(sid.cast::<u8>().add(1)) as usize;
        count <= 15
            && start.checked_add(8 + 4 * count).is_some_and(|n| n <= end)
            && IsValidSid(sid) != 0
    }
}
struct Descriptor(PSECURITY_DESCRIPTOR);
impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: GetSecurityInfo allocates this buffer with LocalAlloc; this owner
        // alone frees it, after all borrowed owner/DACL/ACE pointers have been used.
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn validate(file: &File, directory: bool, user: &User) -> io::Result<()> {
    let handle = file.as_raw_handle();
    // SAFETY: valid open handle, initialized output with its exact Win32 layout.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    unsafe {
        check(GetFileInformationByHandle(handle, &mut info))?;
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
    {
        return Err(denied());
    }
    // SAFETY: querying a live handle does not transfer ownership. Reject pipes,
    // consoles and all non-disk objects before treating the object as state.
    if unsafe { GetFileType(handle) } != FILE_TYPE_DISK {
        return Err(denied());
    }
    let mut owner = null_mut();
    let mut acl = null_mut();
    let mut sd = null_mut();
    // SAFETY: all out-pointers are initialized. The returned descriptor owns the
    // owner and ACL pointers and is immediately placed under RAII on success.
    let result = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut sd,
        )
    };
    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let _descriptor = Descriptor(sd);
    if sd.is_null() || owner.is_null() || acl.is_null() {
        return Err(denied());
    }
    // SAFETY: Win32 returned these descriptor-owned pointers for this handle.
    unsafe {
        if IsValidSecurityDescriptor(sd) == 0
            || IsValidSid(owner) == 0
            || EqualSid(owner, user.sid()) == 0
            || IsValidAcl(acl) == 0
        {
            return Err(denied());
        }
    }
    let mut control = 0;
    let mut revision = 0;
    unsafe {
        check(GetSecurityDescriptorControl(
            sd,
            &mut control,
            &mut revision,
        ))?;
    }
    // Protected DACLs cannot acquire additional inherited grants later.
    if control & SE_DACL_PROTECTED == 0 {
        return Err(denied());
    }
    let mut information: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    // SAFETY: live, validated ACL and correctly sized output storage.
    unsafe {
        check(GetAclInformation(
            acl,
            (&mut information as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        ))?;
    }
    if information.AceCount == 0 || information.AclBytesInUse < size_of::<ACL>() as u32 {
        return Err(denied());
    }
    let end = (acl as usize)
        .checked_add(information.AclBytesInUse as usize)
        .ok_or_else(denied)?;
    for index in 0..information.AceCount {
        let mut raw = null_mut();
        // SAFETY: index is bounded by the validated ACL's reported entry count.
        unsafe {
            check(GetAce(acl, index, &mut raw))?;
        }
        let start = raw as usize;
        if start < (acl as usize) + size_of::<ACL>()
            || start
                .checked_add(size_of::<ACCESS_ALLOWED_ACE>())
                .is_none_or(|n| n > end)
        {
            return Err(denied());
        }
        // SAFETY: the ACE prefix fits inside the validated ACL. We accept only an
        // ordinary allow ACE; extended/object/callback ACE forms fail closed.
        let ace = unsafe { &*raw.cast::<ACCESS_ALLOWED_ACE>() };
        let ace_end = start
            .checked_add(ace.Header.AceSize as usize)
            .ok_or_else(denied)?;
        if ace.Header.AceType != 0 || ace_end > end || ace.Header.AceSize < 16 {
            return Err(denied());
        }
        let sid = (&ace.SidStart as *const u32).cast_mut().cast();
        if !sid_fits(sid, ace_end) {
            return Err(denied());
        }
        // SAFETY: both SIDs passed size/validity checks and remain borrowed from their
        // respective owned allocations. Any grant to a different SID is rejected.
        if unsafe { EqualSid(sid, user.sid()) } == 0 {
            return Err(denied());
        }
    }
    Ok(())
}
fn with_attributes<T>(
    user: &User,
    f: impl FnOnce(&SECURITY_ATTRIBUTES) -> io::Result<T>,
) -> io::Result<T> {
    // Fixed 256-byte ACL storage is aligned for ACL/u32. A single maximum-length
    // Windows SID and ordinary ACE fit; AddAccessAllowedAce also checks capacity.
    let mut acl_storage = [0u32; 64];
    let acl = acl_storage.as_mut_ptr().cast::<ACL>();
    let mut sd: SECURITY_DESCRIPTOR = unsafe { zeroed() };
    // SAFETY: stack allocations have correct layouts, alignment and lifetimes.
    // The descriptor borrows ACL and SID only until f returns. No inheritable
    // handle or inheritable ACE is created, and no caller retains these pointers.
    unsafe {
        check(InitializeAcl(
            acl,
            size_of_val(&acl_storage) as u32,
            ACL_REVISION,
        ))?;
        check(AddAccessAllowedAce(
            acl,
            ACL_REVISION,
            FILE_ALL_ACCESS,
            user.sid(),
        ))?;
        check(InitializeSecurityDescriptor(
            (&mut sd as *mut SECURITY_DESCRIPTOR).cast(),
            1,
        ))?;
        check(SetSecurityDescriptorOwner(
            (&mut sd as *mut SECURITY_DESCRIPTOR).cast(),
            user.sid(),
            0,
        ))?;
        check(SetSecurityDescriptorDacl(
            (&mut sd as *mut SECURITY_DESCRIPTOR).cast(),
            1,
            acl,
            0,
        ))?;
        check(SetSecurityDescriptorControl(
            (&mut sd as *mut SECURITY_DESCRIPTOR).cast(),
            SE_DACL_PROTECTED,
            SE_DACL_PROTECTED,
        ))?;
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: (&mut sd as *mut SECURITY_DESCRIPTOR).cast(),
        bInheritHandle: 0,
    };
    f(&attributes)
}
fn open_handle(
    path: &[u16],
    attributes: Option<&SECURITY_ATTRIBUTES>,
    disposition: u32,
    directory: bool,
    write: bool,
) -> io::Result<File> {
    // SAFETY: NUL-terminated borrowed UTF-16, optional descriptor lives throughout
    // call, no template handle. On success File receives sole handle ownership.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            if directory {
                READ_CONTROL | FILE_READ_ATTRIBUTES
            } else {
                GENERIC_READ | (if write { GENERIC_WRITE } else { 0 }) | READ_CONTROL
            },
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            attributes.map_or(null(), |a| a),
            disposition,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}
pub fn directory(path: &Path) -> io::Result<()> {
    let path = wide(path)?;
    let user = User::current()?;
    with_attributes(&user, |attributes| {
        // SAFETY: path and creation descriptor remain valid through the call. ACL is
        // applied at creation, then independently checked BEFORE any token is stored.
        if unsafe { CreateDirectoryW(path.as_ptr(), attributes) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
                return Err(error);
            }
        }
        let file = open_handle(&path, None, OPEN_EXISTING, true, false)?;
        validate(&file, true, &user)
    })
}
pub fn open(path: &Path, create: bool, exclusive: bool) -> io::Result<File> {
    let path = wide(path)?;
    let user = User::current()?;
    with_attributes(&user, |attributes| {
        let disposition = if exclusive {
            CREATE_NEW
        } else if create {
            OPEN_ALWAYS
        } else {
            OPEN_EXISTING
        };
        let file = open_handle(&path, Some(attributes), disposition, false, true)?;
        validate(&file, false, &user)?;
        Ok(file)
    })
}

pub fn read(path: &Path) -> io::Result<File> {
    let path = wide(path)?;
    let user = User::current()?;
    let file = open_handle(&path, None, OPEN_EXISTING, false, false)?;
    validate(&file, false, &user)?;
    Ok(file)
}

/// Replace an existing file after the caller has retained protected original
/// and replacement recovery paths for every ambiguous failure outcome.
pub fn replace_file(replacement: &Path, destination: &Path) -> io::Result<()> {
    let replacement = wide(replacement)?;
    let destination = wide(destination)?;
    // SAFETY: both path buffers are NUL-terminated and live through the call.
    // Recovery files are owned by the caller; no Win32 backup path is requested.
    unsafe {
        check(ReplaceFileW(
            destination.as_ptr(),
            replacement.as_ptr(),
            null(),
            0,
            null_mut(),
            null_mut(),
        ))
    }
}
