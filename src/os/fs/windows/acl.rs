use std::ffi::c_void;
use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::ops::BitOr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, GENERIC_ALL, GENERIC_EXECUTE, GENERIC_READ, GENERIC_WRITE, HANDLE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT, SetSecurityInfo};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_REVISION, AddAccessAllowedAceEx, CONTAINER_INHERIT_ACE, CopySid,
    CreateWellKnownSid, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetSidIdentifierAuthority, GetTokenInformation,
    INHERIT_ONLY_ACE, InitializeAcl, InitializeSecurityDescriptor, IsWellKnownSid, OBJECT_INHERIT_ACE,
    OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED,
    SECURITY_APP_PACKAGE_AUTHORITY, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SECURITY_MAX_SID_SIZE,
    SetSecurityDescriptorControl, SetSecurityDescriptorDacl, SetSecurityDescriptorOwner, TOKEN_QUERY, TOKEN_USER,
    TokenUser, WELL_KNOWN_SID_TYPE, WinAnonymousSid, WinBuiltinAdministratorsSid, WinCreatorOwnerRightsSid,
    WinCreatorOwnerSid, WinLocalSystemSid, WinWorldSid,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, DELETE, FILE_ALL_ACCESS, FILE_APPEND_DATA, FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_WRITE_ATTRIBUTES,
    FILE_WRITE_DATA, FILE_WRITE_EA, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_ACE_TYPE, ACCESS_DENIED_ACE_TYPE, ACCESS_DENIED_CALLBACK_ACE_TYPE,
    ACCESS_DENIED_CALLBACK_OBJECT_ACE_TYPE, ACCESS_DENIED_OBJECT_ACE_TYPE, ACCESS_SYSTEM_SECURITY, MAXIMUM_ALLOWED,
    SECURITY_DESCRIPTOR_REVISION,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::{ANYONE_MAY_WRITE, NOBODY, uid};

const PRIVATE: u32 = 0o700;
const OTHERS_READ: u32 = 0o055;
const OTHERS_WRITE: u32 = 0o077;
const WRITES: u32 = FILE_WRITE_DATA
    | FILE_APPEND_DATA
    | FILE_WRITE_EA
    | FILE_DELETE_CHILD
    | FILE_WRITE_ATTRIBUTES
    | DELETE
    | WRITE_DAC
    | WRITE_OWNER
    | GENERIC_WRITE
    | GENERIC_ALL
    | MAXIMUM_ALLOWED
    | ACCESS_SYSTEM_SECURITY;
const SID_WORDS: usize = 17;
const ACL_WORDS: usize = 64;
const TOKEN_WORDS: usize = 16;

#[derive(Clone, Copy)]
struct Sid([u32; SID_WORDS]);

impl Sid {
    fn copied(from: PSID) -> io::Result<Self> {
        let mut sid = Self([0; SID_WORDS]);
        ok(unsafe { CopySid(SECURITY_MAX_SID_SIZE, sid.0.as_mut_ptr().cast(), from) })?;
        Ok(sid)
    }

    fn well_known(kind: WELL_KNOWN_SID_TYPE) -> io::Result<Self> {
        let mut sid = Self([0; SID_WORDS]);
        let mut size = SECURITY_MAX_SID_SIZE;
        ok(unsafe { CreateWellKnownSid(kind, null_mut(), sid.0.as_mut_ptr().cast(), &raw mut size) })?;
        Ok(sid)
    }

    fn ptr(&self) -> PSID {
        self.0.as_ptr().cast_mut().cast()
    }

    fn is(&self, other: PSID) -> bool {
        unsafe { EqualSid(self.ptr(), other) != 0 }
    }
}

struct Acl([u32; ACL_WORDS]);

impl Acl {
    fn of(entries: &[(Sid, u32)]) -> io::Result<Self> {
        let mut acl = Self([0; ACL_WORDS]);
        ok(unsafe { InitializeAcl(acl.ptr(), bytes_of::<Self>(), ACL_REVISION) })?;
        let inherited = OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE;
        for (sid, access) in entries {
            ok(unsafe { AddAccessAllowedAceEx(acl.ptr(), ACL_REVISION, inherited, *access, sid.ptr()) })?;
        }
        Ok(acl)
    }

    fn private(user: Sid) -> io::Result<Self> {
        let system = Sid::well_known(WinLocalSystemSid)?;
        let administrators = Sid::well_known(WinBuiltinAdministratorsSid)?;
        Self::of(&[(user, FILE_ALL_ACCESS), (system, FILE_ALL_ACCESS), (administrators, FILE_ALL_ACCESS)])
    }

    fn ptr(&mut self) -> *mut ACL {
        self.0.as_mut_ptr().cast()
    }
}

enum Ace {
    Allows(Rule),
    Denies(Rule),
    DeniesSometimes,
    Unknown,
}

struct Rule {
    sid: PSID,
    mask: u32,
    here: bool,
    files: bool,
}

struct Descriptor {
    memory: PSECURITY_DESCRIPTOR,
    owner: PSID,
    dacl: *mut ACL,
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(self.memory) };
    }
}

pub(crate) fn owner_and_mode(path: &Path, _meta: &Metadata) -> (u32, u32) {
    read_owner_and_mode(|| descriptor(path))
}

pub(crate) fn file_owner_and_mode(file: &File, _meta: &Metadata) -> (u32, u32) {
    read_owner_and_mode(|| descriptor_of(file))
}

fn read_owner_and_mode(read: impl FnOnce() -> io::Result<Descriptor>) -> (u32, u32) {
    match user().and_then(|user| Ok((user, read()?))) {
        Ok((user, read)) => (owner(&user, read.owner), PRIVATE | others(&user, read.dacl)),
        Err(_) => (NOBODY, ANYONE_MAY_WRITE),
    }
}

pub(crate) fn make_private(path: &Path) -> io::Result<()> {
    set_dacl(path, &mut Acl::private(user()?)?)
}

pub(crate) fn create_private_dir_all(dir: &Path) -> io::Result<()> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    match create_private_dir(dir) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(_) if dir.is_dir() => return Ok(()),
        Err(e) => return Err(e),
    }
    if let Some(parent) = dir.parent() {
        create_private_dir_all(parent)?;
    }
    match create_private_dir(dir) {
        Err(_) if dir.is_dir() => Ok(()),
        created => created,
    }
}

pub(crate) fn process_uid(pid: u32) -> io::Result<u32> {
    let user = user()?;
    match user_of_process(pid) {
        Ok(theirs) => Ok(if user.is(theirs.ptr()) { uid() } else { NOBODY }),
        Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED.cast_signed()) => Ok(NOBODY),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
pub(crate) fn share_with_everyone(path: &Path, access: u32) -> io::Result<()> {
    let everyone = Sid::well_known(WinWorldSid)?;
    set_dacl(path, &mut Acl::of(&[(user()?, FILE_ALL_ACCESS), (everyone, access)])?)
}

fn user() -> io::Result<Sid> {
    user_of(unsafe { GetCurrentProcess() })
}

fn user_of_process(pid: u32) -> io::Result<Sid> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    user_of(process.as_raw_handle())
}

fn user_of(process: HANDLE) -> io::Result<Sid> {
    let mut token = null_mut();
    ok(unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) })?;
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut buffer = [0u64; TOKEN_WORDS];
    let (size, mut written) = (bytes_of::<[u64; TOKEN_WORDS]>(), 0);
    let into = buffer.as_mut_ptr().cast();
    ok(unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, into, size, &raw mut written) })?;
    let user = unsafe { buffer.as_ptr().cast::<TOKEN_USER>().read() };
    Sid::copied(user.User.Sid)
}

fn owner(user: &Sid, owner: PSID) -> u32 {
    if !owner.is_null() && ours(user, owner) { uid() } else { NOBODY }
}

fn ours(user: &Sid, sid: PSID) -> bool {
    user.is(sid) || known(sid, WinLocalSystemSid) || known(sid, WinBuiltinAdministratorsSid)
}

fn harmless(user: &Sid, sid: PSID) -> bool {
    ours(user, sid) || known(sid, WinCreatorOwnerSid) || known(sid, WinCreatorOwnerRightsSid) || app_package(sid)
}

fn known(sid: PSID, kind: WELL_KNOWN_SID_TYPE) -> bool {
    unsafe { IsWellKnownSid(sid, kind) != 0 }
}

fn app_package(sid: PSID) -> bool {
    let authority = unsafe { GetSidIdentifierAuthority(sid) };
    !authority.is_null() && unsafe { (*authority).Value } == SECURITY_APP_PACKAGE_AUTHORITY.Value
}

fn others(user: &Sid, dacl: *const ACL) -> u32 {
    if dacl.is_null() {
        return OTHERS_WRITE;
    }
    let count = unsafe { (*dacl).AceCount };
    let aces: Vec<Ace> = (0..u32::from(count)).map(|index| ace(dacl, index)).collect();
    let granted = aces.iter().enumerate().map(|(index, ace)| match ace {
        Ace::Allows(rule) => allowed(user, rule, &aces[..index]),
        Ace::Denies(_) | Ace::DeniesSometimes => 0,
        Ace::Unknown => OTHERS_WRITE,
    });
    granted.fold(0, BitOr::bitor)
}

fn ace(dacl: *const ACL, index: u32) -> Ace {
    let mut ace: *mut c_void = null_mut();
    if unsafe { GetAce(dacl, index, &raw mut ace) } == 0 {
        return Ace::Unknown;
    }
    let header = unsafe { ace.cast::<ACE_HEADER>().read() };
    match u32::from(header.AceType) {
        ACCESS_ALLOWED_ACE_TYPE | ACCESS_ALLOWED_CALLBACK_ACE_TYPE => Ace::Allows(rule(ace.cast(), header.AceFlags)),
        ACCESS_DENIED_ACE_TYPE => Ace::Denies(rule(ace.cast(), header.AceFlags)),
        ACCESS_DENIED_CALLBACK_ACE_TYPE | ACCESS_DENIED_OBJECT_ACE_TYPE | ACCESS_DENIED_CALLBACK_OBJECT_ACE_TYPE => {
            Ace::DeniesSometimes
        }
        _ => Ace::Unknown,
    }
}

fn rule(ace: *const ACCESS_ALLOWED_ACE, flags: u8) -> Rule {
    let flags = u32::from(flags);
    Rule {
        sid: unsafe { &raw const (*ace).SidStart }.cast_mut().cast(),
        mask: mapped(unsafe { (*ace).Mask }),
        here: flags & INHERIT_ONLY_ACE == 0,
        files: flags & OBJECT_INHERIT_ACE != 0,
    }
}

fn mapped(mask: u32) -> u32 {
    let generic = [
        (GENERIC_READ, FILE_GENERIC_READ),
        (GENERIC_WRITE, FILE_GENERIC_WRITE),
        (GENERIC_EXECUTE, FILE_GENERIC_EXECUTE),
        (GENERIC_ALL, FILE_ALL_ACCESS),
    ];
    generic.iter().filter(|(bit, _)| mask & bit != 0).fold(mask, |mapped, (bit, rights)| (mapped & !bit) | rights)
}

fn allowed(user: &Sid, rule: &Rule, earlier: &[Ace]) -> u32 {
    if harmless(user, rule.sid) || !(rule.here || rule.files) {
        return 0;
    }
    let mask = rule.mask & !denied(rule, earlier);
    if mask & WRITES != 0 {
        OTHERS_WRITE
    } else if mask != 0 {
        OTHERS_READ
    } else {
        0
    }
}

fn denied(allow: &Rule, earlier: &[Ace]) -> u32 {
    let at = |reaches: fn(&Rule) -> bool| {
        let covering = earlier.iter().filter_map(|ace| match ace {
            Ace::Denies(deny) if reaches(deny) && names(deny, allow) => Some(deny.mask),
            _ => None,
        });
        covering.fold(0, BitOr::bitor)
    };
    let here = if allow.here { at(|rule| rule.here) } else { u32::MAX };
    let files = if allow.files { at(|rule| rule.files) } else { u32::MAX };
    here & files
}

fn names(deny: &Rule, allow: &Rule) -> bool {
    let everyone = known(deny.sid, WinWorldSid) && !known(allow.sid, WinAnonymousSid);
    everyone || unsafe { EqualSid(deny.sid, allow.sid) != 0 }
}

fn descriptor(path: &Path) -> io::Result<Descriptor> {
    descriptor_of(&opened(path, READ_CONTROL)?)
}

fn descriptor_of(file: &File) -> io::Result<Descriptor> {
    let mut read = Descriptor { memory: null_mut(), owner: null_mut(), dacl: null_mut() };
    let wanted = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let (owner, dacl, memory) = (&raw mut read.owner, &raw mut read.dacl, &raw mut read.memory);
    win32(unsafe {
        GetSecurityInfo(file.as_raw_handle(), SE_FILE_OBJECT, wanted, owner, null_mut(), dacl, null_mut(), memory)
    })?;
    Ok(read)
}

fn set_dacl(path: &Path, acl: &mut Acl) -> io::Result<()> {
    let file = opened(path, READ_CONTROL | WRITE_DAC)?;
    let wanted = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
    win32(unsafe {
        SetSecurityInfo(file.as_raw_handle(), SE_FILE_OBJECT, wanted, null_mut(), null_mut(), acl.ptr(), null())
    })
}

fn opened(path: &Path, access: u32) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.access_mode(access).custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    options.open(path)
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    let user = user()?;
    let mut acl = Acl::private(user)?;
    let mut descriptor = private_descriptor(&user, &mut acl)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: bytes_of::<SECURITY_ATTRIBUTES>(),
        lpSecurityDescriptor: (&raw mut descriptor).cast(),
        bInheritHandle: 0,
    };
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
    ok(unsafe { CreateDirectoryW(wide.as_ptr(), &raw const attributes) })
}

fn private_descriptor(owner: &Sid, dacl: &mut Acl) -> io::Result<SECURITY_DESCRIPTOR> {
    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let pointer: PSECURITY_DESCRIPTOR = (&raw mut descriptor).cast();
    ok(unsafe { InitializeSecurityDescriptor(pointer, SECURITY_DESCRIPTOR_REVISION) })?;
    ok(unsafe { SetSecurityDescriptorOwner(pointer, owner.ptr(), 0) })?;
    ok(unsafe { SetSecurityDescriptorDacl(pointer, 1, dacl.ptr(), 0) })?;
    ok(unsafe { SetSecurityDescriptorControl(pointer, SE_DACL_PROTECTED, SE_DACL_PROTECTED) })?;
    Ok(descriptor)
}

fn bytes_of<T>() -> u32 {
    u32::try_from(size_of::<T>()).unwrap_or(u32::MAX)
}

fn ok(done: i32) -> io::Result<()> {
    if done == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

fn win32(code: u32) -> io::Result<()> {
    if code == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(code.cast_signed())) }
}

#[cfg(test)]
mod tests;
