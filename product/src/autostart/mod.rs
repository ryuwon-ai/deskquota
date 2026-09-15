//! User-login registration templates. Registration is separate from worker lifecycle.
use crate::config::{Auth, StatePaths};
use quick_xml::{
    XmlVersion,
    events::Event,
    name::{Namespace, ResolveResult},
    reader::NsReader,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Macos,
    Linux,
    Windows,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginAuthAvailability {
    Available,
    ConfiguredButUnavailable,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    On,
    Off,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationState {
    Disabled,
    Registered,
    RegisteredBinaryMissing,
    RegisteredBinaryMoved,
    Blocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagerScope {
    User,
    GlobalOrForeign,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ManagerSnapshot {
    Absent,
    Present {
        digest: String,
        owned: bool,
        enabled: bool,
        registered_executable: Option<PathBuf>,
        scope: ManagerScope,
    },
    Unknown {
        reason: String,
    },
}

impl fmt::Display for LoginAuthAvailability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Available => "available",
            Self::ConfiguredButUnavailable => "configured_but_unavailable",
            Self::Unknown => "unknown",
        })
    }
}

pub fn login_auth_availability(auth: &Auth) -> LoginAuthAvailability {
    match auth {
        Auth::Env { .. } => LoginAuthAvailability::Unknown,
        Auth::None | Auth::Forward => LoginAuthAvailability::Available,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationSpec {
    platform: Platform,
    executable: std::path::PathBuf,
    config: std::path::PathBuf,
    label: String,
    user_identity: Option<String>,
    definition: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Snapshot {
    Missing,
    Present {
        digest: String,
        owned: bool,
        registered_executable: Option<PathBuf>,
    },
}

#[derive(Clone, Debug)]
pub struct RegistrationPlan {
    spec: RegistrationSpec,
    action: Action,
    target_path: PathBuf,
    snapshot: Snapshot,
    manager_snapshot: ManagerSnapshot,
    state: RegistrationState,
    preview_hash: String,
}

impl RegistrationPlan {
    pub fn prepare(
        platform: Platform,
        executable: &Path,
        config: &Path,
        user_home: &Path,
        action: Action,
    ) -> Result<Self, Error> {
        if !absolute_for(platform, user_home) {
            return Err(Error::message("autostart user home must be absolute"));
        }
        let spec = RegistrationSpec::new(platform, executable, config)?;
        let target_path = target_path(platform, user_home, spec.label());
        Self::prepare_target(spec, target_path, action)
    }

    pub fn prepare_for_windows_user(
        executable: &Path,
        config: &Path,
        user_home: &Path,
        action: Action,
        user_identity: &str,
    ) -> Result<Self, Error> {
        if !absolute_for(Platform::Windows, user_home) {
            return Err(Error::message("autostart user home must be absolute"));
        }
        let spec = RegistrationSpec::new_for_windows_user(executable, config, user_identity)?;
        let target_path = target_path(Platform::Windows, user_home, spec.label());
        Self::prepare_target(spec, target_path, action)
    }

    fn prepare_target(
        spec: RegistrationSpec,
        target_path: PathBuf,
        action: Action,
    ) -> Result<Self, Error> {
        let platform = spec.platform();
        let executable = spec.executable();
        let snapshot = snapshot(&target_path, spec.label(), platform)?;
        let manager_snapshot = observe_manager(&spec, &target_path, &snapshot);
        let state = registration_state(&snapshot, &manager_snapshot, executable);
        let preview_hash = preview_hash(&spec, action, &target_path, &snapshot, &manager_snapshot);
        Ok(Self {
            spec,
            action,
            target_path,
            snapshot,
            manager_snapshot,
            state,
            preview_hash,
        })
    }

    pub fn state(&self) -> RegistrationState {
        self.state
    }

    pub fn registration_absent(&self) -> bool {
        matches!(self.snapshot, Snapshot::Missing)
            && matches!(self.manager_snapshot, ManagerSnapshot::Absent)
    }

    pub fn target_path(&self) -> &Path {
        &self.target_path
    }

    pub fn preview_hash(&self) -> &str {
        &self.preview_hash
    }

    pub fn registered_executable(&self) -> Option<&Path> {
        match &self.manager_snapshot {
            ManagerSnapshot::Present {
                registered_executable,
                ..
            } => registered_executable.as_deref(),
            ManagerSnapshot::Absent | ManagerSnapshot::Unknown { .. } => None,
        }
    }

    pub fn spec(&self) -> &RegistrationSpec {
        &self.spec
    }

    pub fn action(&self) -> Action {
        self.action
    }

    pub fn manager_commands(&self) -> Vec<String> {
        match (self.spec.platform(), self.action) {
            (Platform::Macos, _) => Vec::new(),
            (Platform::Linux, Action::On) => vec![format!(
                "systemctl --user enable {}.service",
                self.spec.label()
            )],
            (Platform::Linux, Action::Off) => {
                let mut commands = Vec::new();
                if !matches!(self.manager_snapshot, ManagerSnapshot::Absent) {
                    commands.push(format!(
                        "systemctl --user disable {}.service",
                        self.spec.label()
                    ));
                }
                commands.push("remove the owned user unit file".to_owned());
                commands.push("systemctl --user daemon-reload".to_owned());
                commands.push(format!(
                    "systemctl --user show {}.service (re-observe remaining influence)",
                    self.spec.label()
                ));
                commands
            }
            (Platform::Windows, Action::On) => vec![format!(
                "schtasks /Create /TN \\\\llmgw\\{} /XML {}{}",
                self.spec.label(),
                quoted_display(&self.target_path),
                if matches!(
                    self.manager_snapshot,
                    ManagerSnapshot::Present { owned: true, .. }
                ) {
                    " /F"
                } else {
                    ""
                }
            )],
            (Platform::Windows, Action::Off) => vec![format!(
                "schtasks /Delete /TN \\\\llmgw\\{} /F",
                self.spec.label()
            )],
        }
    }

    pub fn preview(&self) -> String {
        let commands = self.manager_commands();
        format!(
            "autostart action: {:?}\nplatform: {:?}\nlabel: {}\nuser identity: {}\ntarget: {}\nexecutable: {}\nconfig: {}\nargv: --config <absolute-config> run\ncurrent registration: {:?}\nmanager snapshot: {:?}\nmanager commands: {}\npreview hash: {}",
            self.action,
            self.spec.platform(),
            self.spec.label(),
            self.spec.user_identity().unwrap_or("not_applicable"),
            self.target_path.display(),
            self.spec.executable().display(),
            self.spec.config().display(),
            self.state,
            self.manager_snapshot,
            if commands.is_empty() {
                "none (file install/remove only; no bootstrap or bootout)".to_owned()
            } else {
                commands.join("; ")
            },
            self.preview_hash,
        )
    }

    pub fn apply(&self) -> Result<(), Error> {
        let current = snapshot(&self.target_path, self.spec.label(), self.spec.platform())?;
        let manager_current = observe_manager(&self.spec, &self.target_path, &current);
        if current != self.snapshot || manager_current != self.manager_snapshot {
            return Err(Error::message(
                "autostart registration changed after preview",
            ));
        }
        if matches!(self.state, RegistrationState::Blocked) {
            return Err(Error::message(
                "autostart target is not owned by this llmgw configuration",
            ));
        }
        match self.action {
            Action::On => self.install(),
            Action::Off => self.remove(),
        }
    }

    fn install(&self) -> Result<(), Error> {
        if self.spec.platform() == Platform::Macos
            && matches!(self.snapshot, Snapshot::Present { digest: ref current_digest, owned: true, .. } if current_digest == &digest(self.spec.definition().as_bytes()))
        {
            return Ok(());
        }
        let parent = self
            .target_path
            .parent()
            .ok_or_else(|| Error::message("autostart target has no parent"))?;
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".{}.{}.tmp", self.spec.label(), std::process::id()));
        let mut temporary_file = crate::lifecycle::platform::open(&temporary, true, true)?;
        let write_result = (|| -> io::Result<()> {
            if self.spec.platform() == Platform::Windows {
                // schtasks /Create consumes a UTF-16 file, including its BOM.
                let bytes: Vec<u8> = std::iter::once(0xfeff)
                    .chain(self.spec.definition().encode_utf16())
                    .flat_map(u16::to_le_bytes)
                    .collect();
                temporary_file.write_all(&bytes)?;
            } else {
                temporary_file.write_all(self.spec.definition().as_bytes())?;
            }
            temporary_file.sync_all()
        })();
        drop(temporary_file);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        let backup = parent.join(format!(
            ".{}.{}.previous",
            self.spec.label(),
            std::process::id()
        ));
        let had_previous = self.target_path.exists();
        if had_previous {
            match fs::symlink_metadata(&backup) {
                Ok(_) => {
                    let _ = fs::remove_file(&temporary);
                    return Err(Error::message("autostart backup path already exists"));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    return Err(error.into());
                }
            }
            fs::rename(&self.target_path, &backup)?;
        }
        if let Err(error) = fs::rename(&temporary, &self.target_path) {
            let _ = fs::remove_file(&temporary);
            if had_previous {
                let _ = fs::rename(&backup, &self.target_path);
            }
            return Err(error.into());
        }
        if let Err(error) = install_manager(self) {
            let _ = fs::remove_file(&self.target_path);
            if had_previous {
                let _ = fs::rename(&backup, &self.target_path);
            }
            return Err(error);
        }
        if had_previous {
            fs::remove_file(backup)?;
        }
        Ok(())
    }

    fn remove(&self) -> Result<(), Error> {
        if matches!(self.manager_snapshot, ManagerSnapshot::Absent)
            && matches!(self.snapshot, Snapshot::Missing)
        {
            return Ok(());
        }
        if self.spec.platform() == Platform::Linux {
            return remove_linux_registration(self);
        }
        if !matches!(self.manager_snapshot, ManagerSnapshot::Absent) {
            remove_manager(self)?;
        }
        if self.target_path.exists() {
            fs::remove_file(&self.target_path)?;
        }
        Ok(())
    }
}

impl RegistrationSpec {
    pub fn new(platform: Platform, executable: &Path, config: &Path) -> Result<Self, Error> {
        if platform == Platform::Windows {
            return Err(Error::message(
                "Windows autostart requires the actual current-user SID",
            ));
        }
        Self::new_with_user(platform, executable, config, None)
    }

    pub fn new_for_windows_user(
        executable: &Path,
        config: &Path,
        user_identity: &str,
    ) -> Result<Self, Error> {
        if !valid_windows_sid(user_identity) {
            return Err(Error::message("Windows current-user SID is invalid"));
        }
        Self::new_with_user(Platform::Windows, executable, config, Some(user_identity))
    }

    fn new_with_user(
        platform: Platform,
        executable: &Path,
        config: &Path,
        user_identity: Option<&str>,
    ) -> Result<Self, Error> {
        if !absolute_for(platform, executable) {
            return Err(Error::message("autostart executable path must be absolute"));
        }
        if !absolute_for(platform, config) {
            return Err(Error::message("autostart config path must be absolute"));
        }
        if platform == Platform::Windows
            && (executable.as_os_str().as_encoded_bytes().contains(&b'%')
                || config.as_os_str().as_encoded_bytes().contains(&b'%'))
        {
            return Err(Error::message(
                "Windows autostart is blocked for percent paths until native argument delivery is verified",
            ));
        }
        let label = format!("io.llmgw.gateway.{}", StatePaths::path_hash(config));
        let definition = match platform {
            Platform::Macos => macos_definition(&label, executable, config),
            Platform::Linux => linux_definition(&label, executable, config),
            Platform::Windows => windows_definition(
                &label,
                executable,
                config,
                user_identity.expect("Windows identity checked above"),
            ),
        };
        Ok(Self {
            platform,
            executable: executable.to_owned(),
            config: config.to_owned(),
            label,
            user_identity: user_identity.map(str::to_owned),
            definition,
        })
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn definition(&self) -> &str {
        &self.definition
    }

    pub fn user_identity(&self) -> Option<&str> {
        self.user_identity.as_deref()
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn argv(&self) -> [&Path; 4] {
        [
            &self.executable,
            Path::new("--config"),
            &self.config,
            Path::new("run"),
        ]
    }
}

#[derive(Debug)]
pub struct Error(String);

impl Error {
    fn message(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self(value.to_string())
    }
}

fn target_path(platform: Platform, user_home: &Path, label: &str) -> PathBuf {
    match platform {
        Platform::Macos => user_home
            .join("Library/LaunchAgents")
            .join(format!("{label}.plist")),
        Platform::Linux => user_home
            .join(".config/systemd/user")
            .join(format!("{label}.service")),
        Platform::Windows => user_home
            .join("AppData/Local/llmgw/autostart")
            .join(format!("{label}.xml")),
    }
}

fn quoted_display(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

fn digest(bytes: &[u8]) -> String {
    let bytes = Sha256::digest(bytes);
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn snapshot(path: &Path, label: &str, platform: Platform) -> Result<Snapshot, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Ok(Snapshot::Present {
                    digest: "non_regular".to_owned(),
                    owned: false,
                    registered_executable: None,
                });
            }
            let bytes = fs::read(path)?;
            let definition = if platform == Platform::Windows {
                decode_windows_xml(&bytes)?
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            };
            let marker = format!("llmgw-owned:{label}");
            Ok(Snapshot::Present {
                digest: digest(&bytes),
                owned: definition.contains(&marker),
                registered_executable: registered_executable(platform, &definition),
            })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Snapshot::Missing),
        Err(error) => Err(error.into()),
    }
}

fn registration_state(
    local: &Snapshot,
    manager: &ManagerSnapshot,
    executable: &Path,
) -> RegistrationState {
    if matches!(local, Snapshot::Present { owned: false, .. }) {
        return RegistrationState::Blocked;
    }
    match manager {
        ManagerSnapshot::Absent => RegistrationState::Disabled,
        ManagerSnapshot::Unknown { .. } => RegistrationState::Blocked,
        ManagerSnapshot::Present { owned: false, .. } => RegistrationState::Blocked,
        ManagerSnapshot::Present { enabled: false, .. } => RegistrationState::Disabled,
        ManagerSnapshot::Present {
            registered_executable: None,
            ..
        } => RegistrationState::Blocked,
        ManagerSnapshot::Present {
            registered_executable: Some(registered),
            ..
        } if !registered.is_file() => RegistrationState::RegisteredBinaryMissing,
        ManagerSnapshot::Present {
            registered_executable: Some(registered),
            ..
        } if registered != executable => RegistrationState::RegisteredBinaryMoved,
        ManagerSnapshot::Present { .. } => RegistrationState::Registered,
    }
}

fn observe_manager(spec: &RegistrationSpec, target: &Path, local: &Snapshot) -> ManagerSnapshot {
    match spec.platform() {
        Platform::Macos => match local {
            Snapshot::Missing => ManagerSnapshot::Absent,
            Snapshot::Present {
                digest,
                owned,
                registered_executable,
            } => ManagerSnapshot::Present {
                digest: digest.clone(),
                owned: *owned,
                enabled: true,
                registered_executable: registered_executable.clone(),
                scope: ManagerScope::User,
            },
        },
        Platform::Linux => observe_linux_manager(target, local, spec.label()),
        Platform::Windows => observe_windows_manager(target, spec),
    }
}

fn observe_linux_manager(target: &Path, local: &Snapshot, label: &str) -> ManagerSnapshot {
    if !cfg!(target_os = "linux") {
        return ManagerSnapshot::Unknown {
            reason: "Linux user manager unavailable on this platform".to_owned(),
        };
    }
    let output = Command::new("systemctl")
        .args([
            "--user",
            "show",
            &format!("{label}.service"),
            "--property=LoadState",
            "--property=UnitFileState",
            "--property=FragmentPath",
            "--property=DropInPaths",
            "--property=NeedDaemonReload",
            "--all",
            "--no-pager",
        ])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            return ManagerSnapshot::Unknown {
                reason: format!("Linux user manager unavailable: {error}"),
            };
        }
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut observed = linux_manager_snapshot_from_show(target, &text);
    if let ManagerSnapshot::Present { owned, .. } = &mut observed {
        *owned &= matches!(local, Snapshot::Present { owned: true, .. });
    }
    if output.status.success() || matches!(observed, ManagerSnapshot::Absent) {
        observed
    } else {
        ManagerSnapshot::Unknown {
            reason: manager_reason(&output, "Linux user manager query failed"),
        }
    }
}

fn linux_manager_snapshot_from_show(target: &Path, output: &str) -> ManagerSnapshot {
    let mut properties = BTreeMap::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once('=') else {
            return ManagerSnapshot::Unknown {
                reason: "Linux user manager returned malformed properties".to_owned(),
            };
        };
        if properties.insert(name, value).is_some() {
            return ManagerSnapshot::Unknown {
                reason: "Linux user manager returned duplicate properties".to_owned(),
            };
        }
    }
    if properties.get("LoadState") == Some(&"not-found") {
        return ManagerSnapshot::Absent;
    }
    if properties.get("LoadState") != Some(&"loaded") {
        return ManagerSnapshot::Unknown {
            reason: "Linux user manager did not report a loaded or absent unit".to_owned(),
        };
    }
    let Some(fragment) = properties
        .get("FragmentPath")
        .filter(|value| !value.is_empty())
    else {
        return ManagerSnapshot::Unknown {
            reason: "Linux user manager omitted the unit fragment path".to_owned(),
        };
    };
    let enabled = match properties.get("UnitFileState").copied() {
        Some("enabled" | "enabled-runtime") => true,
        Some("disabled") => false,
        _ => {
            return ManagerSnapshot::Unknown {
                reason: "Linux user manager returned an unsupported unit-file state".to_owned(),
            };
        }
    };
    let fragment = Path::new(fragment);
    let local_scope = fragment == target;
    if local_scope {
        match properties.get("NeedDaemonReload").copied() {
            Some("no") => {}
            Some("yes") => {
                return ManagerSnapshot::Unknown {
                    reason: "Linux user manager has a pending daemon reload".to_owned(),
                };
            }
            _ => {
                return ManagerSnapshot::Unknown {
                    reason: "Linux user manager omitted a supported daemon-reload state".to_owned(),
                };
            }
        }
        match properties.get("DropInPaths").copied() {
            Some("") => {}
            Some(_) => {
                return ManagerSnapshot::Unknown {
                    reason: "Linux user manager loaded unsupported drop-in configuration"
                        .to_owned(),
                };
            }
            None => {
                return ManagerSnapshot::Unknown {
                    reason: "Linux user manager omitted loaded drop-in paths".to_owned(),
                };
            }
        }
    }
    ManagerSnapshot::Present {
        digest: digest(output.as_bytes()),
        owned: local_scope && target.is_file(),
        enabled,
        registered_executable: if local_scope {
            fs::read_to_string(target)
                .ok()
                .and_then(|definition| registered_executable(Platform::Linux, &definition))
        } else {
            None
        },
        scope: if local_scope {
            ManagerScope::User
        } else {
            ManagerScope::GlobalOrForeign
        },
    }
}

fn observe_windows_manager(target: &Path, spec: &RegistrationSpec) -> ManagerSnapshot {
    if !cfg!(target_os = "windows") {
        return ManagerSnapshot::Unknown {
            reason: "Windows Task Scheduler unavailable on this platform".to_owned(),
        };
    }
    let output = match windows_task_query(spec.label()) {
        Ok(output) => output,
        Err(error) => {
            return ManagerSnapshot::Unknown {
                reason: error.to_string(),
            };
        }
    };
    if !output.status.success() {
        return windows_query_failure(
            output.status.code(),
            &manager_reason(&output, "Windows Task Scheduler query failed"),
        );
    }
    let xml = match decode_windows_xml(&output.stdout) {
        Ok(xml) => xml,
        Err(error) => {
            return ManagerSnapshot::Unknown {
                reason: error.to_string(),
            };
        }
    };
    let mut snapshot = windows_manager_snapshot_from_xml(
        spec.label(),
        target,
        spec.user_identity().unwrap_or_default(),
        &xml,
    );
    if let ManagerSnapshot::Present { digest, .. } = &mut snapshot {
        *digest = digest_bytes(&output.stdout);
    }
    snapshot
}

fn digest_bytes(bytes: &[u8]) -> String {
    digest(bytes)
}

fn windows_query_failure(code: Option<i32>, reason: &str) -> ManagerSnapshot {
    if code.is_some_and(|code| matches!(code as u32, 0x8007_0002 | 0x8007_0003)) {
        ManagerSnapshot::Absent
    } else {
        ManagerSnapshot::Unknown {
            reason: reason.to_owned(),
        }
    }
}

fn decode_windows_xml(bytes: &[u8]) -> Result<String, Error> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little_endian = bytes.starts_with(&[0xff, 0xfe]);
        let body = &bytes[2..];
        if body.len() % 2 != 0 {
            return Err(Error::message("Windows task XML has truncated UTF-16"));
        }
        let words = body.chunks_exact(2).map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        });
        return char::decode_utf16(words)
            .collect::<Result<String, _>>()
            .map_err(|_| Error::message("Windows task XML is not valid UTF-16"));
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|_| Error::message("Windows task XML is not valid UTF-8 or UTF-16"))
}

fn windows_manager_snapshot_from_xml(
    label: &str,
    _target: &Path,
    expected_user: &str,
    definition: &str,
) -> ManagerSnapshot {
    match parse_windows_manager_xml(label, expected_user, definition) {
        Ok((owned, current_user_scoped, enabled, executable)) => ManagerSnapshot::Present {
            digest: digest(definition.as_bytes()),
            owned,
            enabled,
            registered_executable: if owned { executable } else { None },
            scope: if current_user_scoped {
                ManagerScope::User
            } else {
                ManagerScope::GlobalOrForeign
            },
        },
        Err(error) => ManagerSnapshot::Unknown {
            reason: error.to_string(),
        },
    }
}

fn parse_windows_manager_xml(
    label: &str,
    expected_user: &str,
    definition: &str,
) -> Result<(bool, bool, bool, Option<PathBuf>), Error> {
    const TASK_NS: Namespace<'static> =
        Namespace("http://schemas.microsoft.com/windows/2004/02/mit/task");
    let mut reader = NsReader::from_str(definition);
    reader.config_mut().trim_text(false);
    let mut stack = Vec::<String>::new();
    let mut fields = BTreeMap::<String, String>::new();
    let mut shape = WindowsTaskShape::default();
    let mut declaration_allowed = true;
    loop {
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|_| Error::message("Windows task XML is malformed"))?;
        match event {
            Event::Start(start) => {
                if stack.is_empty() {
                    declaration_allowed = false;
                }
                if namespace != ResolveResult::Bound(TASK_NS) {
                    return Err(Error::message(
                        "Windows task XML uses an unexpected namespace",
                    ));
                }
                shape.observe(&stack, start.local_name().as_ref());
                stack.push(start.local_name().as_ref().to_owned());
                if let Some(key) = windows_field_key(&stack)
                    && fields.insert(key.to_owned(), String::new()).is_some()
                {
                    return Err(Error::message(
                        "Windows task XML has duplicate identity fields",
                    ));
                }
            }
            Event::Empty(empty) => {
                if stack.is_empty() {
                    declaration_allowed = false;
                }
                if namespace != ResolveResult::Bound(TASK_NS) {
                    return Err(Error::message(
                        "Windows task XML uses an unexpected namespace",
                    ));
                }
                shape.observe(&stack, empty.local_name().as_ref());
                stack.push(empty.local_name().as_ref().to_owned());
                if let Some(key) = windows_field_key(&stack)
                    && fields.insert(key.to_owned(), String::new()).is_some()
                {
                    return Err(Error::message(
                        "Windows task XML has duplicate identity fields",
                    ));
                }
                stack.pop();
            }
            Event::End(end) => {
                if namespace != ResolveResult::Bound(TASK_NS)
                    || stack.pop().as_deref() != Some(end.local_name().as_ref())
                {
                    return Err(Error::message(
                        "Windows task XML has mismatched task-schema elements",
                    ));
                }
            }
            Event::Text(text) => {
                let value = text.xml_content(XmlVersion::Implicit1_0);
                if stack.is_empty() {
                    declaration_allowed = false;
                    if !value
                        .chars()
                        .all(|character| matches!(character, ' ' | '\t' | '\r' | '\n'))
                    {
                        return Err(Error::message(
                            "Windows task XML has content outside the Task document",
                        ));
                    }
                } else if let Some(key) = windows_field_key(&stack) {
                    fields
                        .get_mut(key)
                        .expect("field initialized with its start element")
                        .push_str(&value);
                }
            }
            Event::GeneralRef(reference) => {
                if stack.is_empty() {
                    return Err(Error::message(
                        "Windows task XML has content outside the Task document",
                    ));
                }
                let resolved = match reference.resolve_char_ref() {
                    Ok(Some(character)) if valid_xml_character(character) => character.to_string(),
                    Ok(Some(_)) | Err(_) => {
                        return Err(Error::message(
                            "Windows task XML has an invalid character reference",
                        ));
                    }
                    Ok(None) => quick_xml::escape::resolve_xml_entity(&reference)
                        .ok_or_else(|| Error::message("Windows task XML uses a custom entity"))?
                        .to_owned(),
                };
                if let Some(key) = windows_field_key(&stack) {
                    fields
                        .get_mut(key)
                        .expect("field initialized with its start element")
                        .push_str(&resolved);
                }
            }
            Event::Decl(_) => {
                if !stack.is_empty() || !declaration_allowed {
                    return Err(Error::message(
                        "Windows task XML has a misplaced declaration",
                    ));
                }
                declaration_allowed = false;
            }
            Event::Comment(_) | Event::PI(_) => {
                if stack.is_empty() {
                    declaration_allowed = false;
                }
            }
            Event::Eof => break,
            Event::DocType(_) | Event::CData(_) => {
                return Err(Error::message(
                    "Windows task XML contains unsupported content",
                ));
            }
        }
    }
    if !stack.is_empty() {
        return Err(Error::message("Windows task XML ended inside an element"));
    }
    shape.validate()?;
    let marker = format!("llmgw-owned:{label}");
    let current_user_scoped = fields
        .get("trigger_user")
        .is_some_and(|user| windows_identity_matches(user.trim(), expected_user))
        && fields
            .get("principal_user")
            .is_some_and(|user| windows_identity_matches(user.trim(), expected_user));
    let owned = fields
        .get("description")
        .is_some_and(|description| description.trim() == marker)
        && current_user_scoped;
    let enabled = match fields.get("enabled").map(String::as_str) {
        // Task Scheduler omits the schema's true default when exporting XML.
        None => true,
        Some(value) if value.trim() == "true" => true,
        Some(value) if value.trim() == "false" => false,
        _ => {
            return Err(Error::message(
                "Windows task XML has invalid Settings/Enabled",
            ));
        }
    };
    if fields
        .get("trigger_enabled")
        .is_some_and(|value| value.trim() != "true")
    {
        return Err(Error::message(
            "Windows task XML has an unsupported logon trigger",
        ));
    }
    let executable = fields
        .get("command")
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| Error::message("Windows task XML omitted Exec/Command"))?;
    Ok((owned, current_user_scoped, enabled, Some(executable)))
}

fn windows_identity_matches(observed: &str, expected_sid: &str) -> bool {
    if observed == expected_sid {
        return true;
    }
    if observed.is_empty() || valid_windows_sid(observed) {
        return false;
    }
    #[cfg(windows)]
    {
        // Task Scheduler may export the logon user as DOMAIN\name. Resolve it
        // through Windows; a display-name comparison cannot establish ownership.
        crate::lifecycle::platform::account_identity(observed).is_ok_and(|sid| sid == expected_sid)
    }
    #[cfg(not(windows))]
    false
}

#[derive(Default)]
struct WindowsTaskShape {
    document_elements: usize,
    task_roots: usize,
    registration_sections: usize,
    trigger_sections: usize,
    trigger_children: usize,
    logon_triggers: usize,
    principal_sections: usize,
    principal_children: usize,
    principals: usize,
    action_sections: usize,
    action_children: usize,
    exec_actions: usize,
    settings_sections: usize,
}

impl WindowsTaskShape {
    fn observe(&mut self, stack: &[String], element: &str) {
        match stack {
            [] => {
                self.document_elements += 1;
                if element == "Task" {
                    self.task_roots += 1;
                }
            }
            [task] if task == "Task" => match element {
                "RegistrationInfo" => self.registration_sections += 1,
                "Triggers" => self.trigger_sections += 1,
                "Principals" => self.principal_sections += 1,
                "Actions" => self.action_sections += 1,
                "Settings" => self.settings_sections += 1,
                _ => {}
            },
            [task, triggers] if task == "Task" && triggers == "Triggers" => {
                self.trigger_children += 1;
                if element == "LogonTrigger" {
                    self.logon_triggers += 1;
                }
            }
            [task, principals] if task == "Task" && principals == "Principals" => {
                self.principal_children += 1;
                if element == "Principal" {
                    self.principals += 1;
                }
            }
            [task, actions] if task == "Task" && actions == "Actions" => {
                self.action_children += 1;
                if element == "Exec" {
                    self.exec_actions += 1;
                }
            }
            _ => {}
        }
    }

    fn validate(&self) -> Result<(), Error> {
        if self.document_elements != 1
            || self.task_roots != 1
            || self.registration_sections != 1
            || self.trigger_sections != 1
            || self.trigger_children != 1
            || self.logon_triggers != 1
            || self.principal_sections != 1
            || self.principal_children != 1
            || self.principals != 1
            || self.action_sections != 1
            || self.action_children != 1
            || self.exec_actions != 1
            || self.settings_sections != 1
        {
            return Err(Error::message(
                "Windows task XML has unsupported task structure",
            ));
        }
        Ok(())
    }
}

fn valid_xml_character(character: char) -> bool {
    matches!(character, '\u{9}' | '\u{a}' | '\u{d}')
        || matches!(character, '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn windows_field_key(stack: &[String]) -> Option<&'static str> {
    match stack {
        [task, registration, description]
            if task == "Task"
                && registration == "RegistrationInfo"
                && description == "Description" =>
        {
            Some("description")
        }
        [task, triggers, trigger, user]
            if task == "Task"
                && triggers == "Triggers"
                && trigger == "LogonTrigger"
                && user == "UserId" =>
        {
            Some("trigger_user")
        }
        [task, triggers, trigger, enabled]
            if task == "Task"
                && triggers == "Triggers"
                && trigger == "LogonTrigger"
                && enabled == "Enabled" =>
        {
            Some("trigger_enabled")
        }
        [task, principals, principal, user]
            if task == "Task"
                && principals == "Principals"
                && principal == "Principal"
                && user == "UserId" =>
        {
            Some("principal_user")
        }
        [task, settings, enabled]
            if task == "Task" && settings == "Settings" && enabled == "Enabled" =>
        {
            Some("enabled")
        }
        [task, actions, exec, command]
            if task == "Task" && actions == "Actions" && exec == "Exec" && command == "Command" =>
        {
            Some("command")
        }
        _ => None,
    }
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

fn between<'a>(value: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let value = value.split_once(start)?.1;
    Some(value.split_once(end)?.0)
}

fn registered_executable(platform: Platform, definition: &str) -> Option<PathBuf> {
    let encoded = match platform {
        Platform::Macos => {
            let arguments = definition.split_once("<key>ProgramArguments</key>")?.1;
            between(arguments, "<string>", "</string>")?
        }
        Platform::Windows => between(definition, "<Command>", "</Command>")?,
        Platform::Linux => {
            let line = definition
                .lines()
                .find_map(|line| line.strip_prefix("ExecStart=:"))?;
            return parse_systemd_first_argument(line).map(PathBuf::from);
        }
    };
    Some(PathBuf::from(xml_unescape(encoded)))
}

fn parse_systemd_first_argument(value: &str) -> Option<String> {
    let value = value.strip_prefix('"')?;
    let mut output = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(output.replace("%%", "%"));
        } else {
            output.push(character);
        }
    }
    None
}

fn preview_hash(
    spec: &RegistrationSpec,
    action: Action,
    target: &Path,
    snapshot: &Snapshot,
    manager_snapshot: &ManagerSnapshot,
) -> String {
    digest(
        format!(
            "version=2\nplatform={:?}\naction={action:?}\nlabel={}\nuser_identity={:?}\ntarget={}\nexecutable={}\nconfig={}\ndefinition={}\nfile_snapshot={snapshot:?}\nmanager_snapshot={manager_snapshot:?}",
            spec.platform(),
            spec.label(),
            spec.user_identity(),
            target.display(),
            spec.executable().display(),
            spec.config().display(),
            spec.definition(),
        )
        .as_bytes(),
    )
}

pub fn current_platform() -> Platform {
    #[cfg(target_os = "macos")]
    return Platform::Macos;
    #[cfg(target_os = "windows")]
    return Platform::Windows;
    #[cfg(all(unix, not(target_os = "macos")))]
    return Platform::Linux;
}

fn current_home() -> Result<PathBuf, Error> {
    #[cfg(target_os = "windows")]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(target_os = "windows"))]
    let home = std::env::var_os("HOME");
    let path = home
        .map(PathBuf::from)
        .ok_or_else(|| Error::message("cannot determine current user home for autostart"))?;
    if !path.is_absolute() {
        return Err(Error::message(
            "current user home for autostart must be absolute",
        ));
    }
    Ok(path)
}

pub fn prepare_current(config: &Path, action: Action) -> Result<RegistrationPlan, Error> {
    let config = fs::canonicalize(config)?;
    let executable = fs::canonicalize(std::env::current_exe()?)?;
    let platform = current_platform();
    let home = current_home()?;
    let spec = if platform == Platform::Windows {
        let identity = crate::lifecycle::platform::current_user_identity()?;
        RegistrationSpec::new_for_windows_user(&executable, &config, &identity)?
    } else {
        RegistrationSpec::new(platform, &executable, &config)?
    };
    let target = match platform {
        Platform::Macos => target_path(platform, &home, spec.label()),
        Platform::Linux => std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"))
            .join("systemd/user")
            .join(format!("{}.service", spec.label())),
        Platform::Windows => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| absolute_for(Platform::Windows, path))
            .unwrap_or_else(|| home.join("AppData/Local"))
            .join("llmgw/autostart")
            .join(format!("{}.xml", spec.label())),
    };
    RegistrationPlan::prepare_target(spec, target, action)
}

pub fn query_current(config: &Path, auth: &Auth) -> Value {
    match prepare_current(config, Action::On) {
        Ok(plan) => {
            let (manager, manager_scope, manager_ownership, reason) = manager_registration(&plan);
            let registration = match (&plan.manager_snapshot, plan.state()) {
                (ManagerSnapshot::Unknown { .. }, _) => "unknown",
                (ManagerSnapshot::Present { owned: false, .. }, _) => "blocked",
                (_, RegistrationState::Disabled) => "disabled",
                (
                    _,
                    RegistrationState::Registered
                    | RegistrationState::RegisteredBinaryMissing
                    | RegistrationState::RegisteredBinaryMoved,
                ) => "registered",
                (_, RegistrationState::Blocked) => "blocked",
            };
            json!({
                "registration": registration,
                "manager": manager,
                "manager_scope": manager_scope,
                "manager_ownership": manager_ownership,
                "reason": reason,
                "label": plan.spec().label(),
                "target": plan.target_path(),
                "executable": plan.spec().executable(),
                "registered_executable": plan.registered_executable(),
                "config": plan.spec().config(),
                "registered_binary_available": plan.registered_executable().is_some_and(Path::is_file),
                "registered_executable_matches_current": plan.registered_executable() == Some(plan.spec().executable()),
                "login_auth_availability": login_auth_availability(auth).to_string(),
                "current_shell_auth_is_login_proof": false,
                "linux_linger": linux_linger(),
            })
        }
        Err(error) => json!({
            "registration":"unknown",
            "manager":"unavailable",
            "manager_scope":"unknown",
            "manager_ownership":"unknown",
            "reason":error.to_string(),
            "login_auth_availability":login_auth_availability(auth).to_string(),
            "current_shell_auth_is_login_proof":false,
        }),
    }
}

fn manager_registration(plan: &RegistrationPlan) -> (String, String, String, Option<String>) {
    let absent = match plan.spec.platform() {
        Platform::Macos => "file_absent",
        Platform::Linux => "unit_absent",
        Platform::Windows => "task_absent",
    };
    match &plan.manager_snapshot {
        ManagerSnapshot::Absent => (
            absent.to_owned(),
            "user".to_owned(),
            "absent".to_owned(),
            None,
        ),
        ManagerSnapshot::Unknown { reason } => (
            "unknown".to_owned(),
            "unknown".to_owned(),
            "unknown".to_owned(),
            Some(reason.clone()),
        ),
        ManagerSnapshot::Present {
            owned,
            enabled,
            scope,
            ..
        } => {
            let scope = match scope {
                ManagerScope::User => "user",
                ManagerScope::GlobalOrForeign => "global_or_foreign",
            };
            if !owned {
                return (
                    "foreign".to_owned(),
                    scope.to_owned(),
                    "foreign".to_owned(),
                    Some(
                        "manager registration is not owned by this llmgw configuration".to_owned(),
                    ),
                );
            }
            (
                if plan.spec.platform() == Platform::Macos {
                    "file_installed"
                } else if *enabled {
                    "enabled"
                } else {
                    "disabled"
                }
                .to_owned(),
                scope.to_owned(),
                "owned".to_owned(),
                None,
            )
        }
    }
}

fn manager_reason(output: &std::process::Output, fallback: &str) -> String {
    match output.status.code() {
        Some(code) => format!("{fallback} (exit status {code})"),
        None => format!("{fallback} (terminated without an exit status)"),
    }
}

fn linux_linger() -> Value {
    if current_platform() != Platform::Linux {
        return Value::Null;
    }
    match std::env::var_os("USER") {
        Some(user) => json!({
            "enabled": Path::new("/var/lib/systemd/linger").join(user).exists(),
            "changed_by_llmgw": false,
        }),
        None => json!({"enabled":"unknown", "changed_by_llmgw":false}),
    }
}

fn run_manager(mut command: Command, purpose: &str) -> Result<(), Error> {
    let output = command
        .output()
        .map_err(|error| Error::message(format!("{purpose} unavailable: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(Error::message(match output.status.code() {
        Some(code) => format!("{purpose} blocked (exit status {code})"),
        None => format!("{purpose} blocked (terminated without an exit status)"),
    }))
}

fn remove_linux_registration(plan: &RegistrationPlan) -> Result<(), Error> {
    if !cfg!(target_os = "linux") {
        return Err(Error::message(
            "Linux user manager unavailable on this platform",
        ));
    }
    if !matches!(plan.manager_snapshot, ManagerSnapshot::Absent) {
        remove_manager(plan)?;
    }
    if plan.target_path().exists() {
        fs::remove_file(plan.target_path())?;
    }
    let mut reload = Command::new("systemctl");
    reload.args(["--user", "daemon-reload"]);
    run_manager(reload, "Linux user manager reload").map_err(|error| {
        Error::message(format!(
            "local user registration was removed, but manager reload failed: {error}"
        ))
    })?;
    let local = snapshot(plan.target_path(), plan.spec.label(), Platform::Linux)?;
    linux_post_remove_result(observe_linux_manager(
        plan.target_path(),
        &local,
        plan.spec.label(),
    ))
}

fn linux_post_remove_result(manager: ManagerSnapshot) -> Result<(), Error> {
    match manager {
        ManagerSnapshot::Absent => Ok(()),
        ManagerSnapshot::Present { .. } => Err(Error::message(
            "local user registration was removed, but global or foreign manager influence remains",
        )),
        ManagerSnapshot::Unknown { reason } => Err(Error::message(format!(
            "local user registration was removed, but the remaining manager state is unknown: {reason}"
        ))),
    }
}

fn windows_task_query(label: &str) -> Result<std::process::Output, Error> {
    let mut command = Command::new("schtasks");
    command.args(["/Query", "/TN"]);
    command.arg(format!("\\llmgw\\{label}"));
    command.args(["/XML", "/HRESULT"]);
    command
        .output()
        .map_err(|error| Error::message(format!("Windows Task Scheduler unavailable: {error}")))
}

fn install_manager(plan: &RegistrationPlan) -> Result<(), Error> {
    match plan.spec.platform() {
        Platform::Macos => Ok(()),
        Platform::Linux => {
            if !cfg!(target_os = "linux") {
                return Err(Error::message(
                    "Linux user manager unavailable on this platform",
                ));
            }
            let mut command = Command::new("systemctl");
            command.args(["--user", "enable"]);
            command.arg(format!("{}.service", plan.spec.label()));
            run_manager(command, "Linux user manager")
        }
        Platform::Windows => {
            if !cfg!(target_os = "windows") {
                return Err(Error::message(
                    "Windows Task Scheduler unavailable on this platform",
                ));
            }
            let overwrite = match plan.manager_snapshot {
                ManagerSnapshot::Absent => false,
                ManagerSnapshot::Present { owned: true, .. } => true,
                ManagerSnapshot::Present { owned: false, .. } => {
                    return Err(Error::message(
                        "Windows task label collision is not owned by this llmgw configuration",
                    ));
                }
                ManagerSnapshot::Unknown { .. } => {
                    return Err(Error::message(
                        "Windows Task Scheduler state is unknown; registration is blocked",
                    ));
                }
            };
            let mut command = Command::new("schtasks");
            command.args(["/Create", "/TN"]);
            command.arg(format!("\\llmgw\\{}", plan.spec.label()));
            command.arg("/XML").arg(plan.target_path());
            if overwrite {
                command.arg("/F");
            }
            run_manager(command, "Windows Task Scheduler")
        }
    }
}

fn remove_manager(plan: &RegistrationPlan) -> Result<(), Error> {
    match plan.spec.platform() {
        Platform::Macos => Ok(()),
        Platform::Linux => {
            if !cfg!(target_os = "linux") {
                return Err(Error::message(
                    "Linux user manager unavailable on this platform",
                ));
            }
            let mut command = Command::new("systemctl");
            command.args(["--user", "disable"]);
            command.arg(format!("{}.service", plan.spec.label()));
            run_manager(command, "Linux user manager")
        }
        Platform::Windows => {
            if !cfg!(target_os = "windows") {
                return Err(Error::message(
                    "Windows Task Scheduler unavailable on this platform",
                ));
            }
            match plan.manager_snapshot {
                ManagerSnapshot::Present { owned: true, .. } => {}
                ManagerSnapshot::Present { owned: false, .. } => {
                    return Err(Error::message(
                        "Windows task label collision is not owned by this llmgw configuration",
                    ));
                }
                ManagerSnapshot::Absent => return Ok(()),
                ManagerSnapshot::Unknown { .. } => {
                    return Err(Error::message(
                        "Windows Task Scheduler state is unknown; removal is blocked",
                    ));
                }
            }
            let mut command = Command::new("schtasks");
            command.args(["/Delete", "/TN"]);
            command.arg(format!("\\llmgw\\{}", plan.spec.label()));
            command.arg("/F");
            run_manager(command, "Windows Task Scheduler")
        }
    }
}

fn absolute_for(platform: Platform, path: &Path) -> bool {
    let bytes = path.as_os_str().as_encoded_bytes();
    if platform != Platform::Windows {
        return bytes.starts_with(b"/");
    }
    if cfg!(windows) && path.is_absolute() {
        return true;
    }
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn valid_windows_sid(identity: &str) -> bool {
    let mut parts = identity.split('-');
    parts.next() == Some("S")
        && parts.clone().count() >= 2
        && parts.all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn xml(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(character),
        }
    }
    output
}

fn macos_definition(label: &str, executable: &Path, config: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
    <key>Label</key>\n\
    <string>{}</string>\n\
    <key>LLMGWOwner</key>\n\
    <string>llmgw-owned:{}</string>\n\
    <key>ProgramArguments</key>\n\
    <array>\n\
        <string>{}</string>\n\
        <string>--config</string>\n\
        <string>{}</string>\n\
        <string>run</string>\n\
    </array>\n\
    <key>RunAtLoad</key>\n\
    <true/>\n\
    <key>KeepAlive</key>\n\
    <false/>\n\
</dict>\n\
</plist>\n",
        xml(label),
        xml(label),
        xml(&executable.to_string_lossy()),
        xml(&config.to_string_lossy()),
    )
}

fn systemd_arg(path: &Path) -> String {
    let value = path.to_string_lossy().replace('%', "%%");
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '\\' | '"' => {
                output.push('\\');
                output.push(character);
            }
            _ => output.push(character),
        }
    }
    output.push('"');
    output
}

fn linux_definition(label: &str, executable: &Path, config: &Path) -> String {
    format!(
        "# llmgw-owned:{label}\n[Unit]\nDescription=llmgw user gateway ({label})\n\n\
[Service]\nType=simple\nExecStart=:{} --config {} run\nRestart=no\n\n\
[Install]\nWantedBy=default.target\n",
        systemd_arg(executable),
        systemd_arg(config),
    )
}

fn windows_argument(path: &Path) -> String {
    let value = path.to_string_lossy();
    let mut output = String::from("\"");
    let mut backslashes = 0;
    for character in value.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }
        if character == '"' {
            output.push_str(&"\\".repeat(backslashes * 2 + 1));
            output.push('"');
        } else {
            output.push_str(&"\\".repeat(backslashes));
            output.push(character);
        }
        backslashes = 0;
    }
    output.push_str(&"\\".repeat(backslashes * 2));
    output.push('"');
    output
}

fn windows_definition(
    label: &str,
    executable: &Path,
    config: &Path,
    user_identity: &str,
) -> String {
    let arguments = format!("--config {} run", windows_argument(config));
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n\
<Task version=\"1.4\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n\
  <RegistrationInfo><Description>llmgw-owned:{}</Description><URI>\\llmgw\\{}</URI></RegistrationInfo>\n\
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{}</UserId></LogonTrigger></Triggers>\n\
  <Principals><Principal id=\"CurrentUser\"><UserId>{}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>\n\
  <Settings>\n\
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n\
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n\
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n\
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>\n\
    <RunOnlyIfIdle>false</RunOnlyIfIdle>\n\
    <WakeToRun>false</WakeToRun>\n\
    <Enabled>true</Enabled>\n\
    <Hidden>false</Hidden>\n\
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n\
  </Settings>\n\
  <Actions Context=\"CurrentUser\"><Exec><Command>{}</Command><Arguments>{}</Arguments></Exec></Actions>\n\
</Task>\n",
        xml(label),
        xml(label),
        xml(user_identity),
        xml(user_identity),
        xml(&executable.to_string_lossy()),
        xml(&arguments),
    )
}

#[cfg(test)]
mod manager_snapshot_tests {
    use super::*;

    #[test]
    fn windows_manager_definition_distinguishes_disabled_and_foreign_tasks() {
        let label = "io.llmgw.gateway.fixture";
        let target = Path::new(r"C:\Users\fixture\task.xml");
        let user = "S-1-5-21-1-2-3-1001";
        let owned = format!(
            "<Task xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"><RegistrationInfo><Description>llmgw-owned:{label}</Description></RegistrationInfo><Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers><Principals><Principal><UserId>{user}</UserId></Principal></Principals><Settings><Enabled>false</Enabled></Settings><Actions><Exec><Command>C:\\llmgw &amp; gateway.exe</Command></Exec></Actions></Task>"
        );
        let disabled = windows_manager_snapshot_from_xml(label, target, user, &owned);
        assert!(matches!(
            disabled,
            ManagerSnapshot::Present {
                owned: true,
                enabled: false,
                ..
            }
        ));
        assert_eq!(
            match disabled {
                ManagerSnapshot::Present {
                    registered_executable,
                    ..
                } => registered_executable,
                _ => None,
            },
            Some(PathBuf::from(r"C:\llmgw & gateway.exe"))
        );

        let foreign = windows_manager_snapshot_from_xml(
            label,
            target,
            user,
            &owned.replace("llmgw-owned:", "foreign-owner:"),
        );
        assert!(matches!(
            foreign,
            ManagerSnapshot::Present { owned: false, .. }
        ));

        let unscoped = owned.replace(
            &format!("<UserId>{user}</UserId></LogonTrigger>"),
            "</LogonTrigger>",
        );
        assert!(matches!(
            windows_manager_snapshot_from_xml(label, target, user, &unscoped),
            ManagerSnapshot::Present {
                owned: false,
                scope: ManagerScope::GlobalOrForeign,
                ..
            }
        ));
    }

    #[test]
    fn windows_manager_accepts_numeric_refs_and_rejects_unsupported_refs() {
        let label = "io.llmgw.gateway.numeric";
        let user = "S-1-5-21-1-2-3-1001";
        let definition = windows_definition(
            label,
            Path::new(r"C:\Tools\llmgw & gateway.exe"),
            Path::new(r"C:\config.toml"),
            user,
        );
        let numeric = definition
            .replace("&amp;", "&#38;")
            .replace(user, &user.replace('-', "&#x2d;"));
        assert_eq!(
            parse_windows_manager_xml(label, user, &numeric).unwrap(),
            parse_windows_manager_xml(label, user, &definition).unwrap()
        );

        for unsupported in ["&custom;", "&#x110000;", "&#0;"] {
            let changed = definition.replace("llmgw-owned", unsupported);
            assert!(parse_windows_manager_xml(label, user, &changed).is_err());
        }
        let custom_in_untracked_field = definition.replace(
            &format!("<URI>\\llmgw\\{label}</URI>"),
            "<URI>&custom;</URI>",
        );
        assert!(parse_windows_manager_xml(label, user, &custom_in_untracked_field).is_err());
    }

    #[test]
    fn windows_manager_accepts_scheduler_omitted_enabled_defaults() {
        let label = "io.llmgw.gateway.defaults";
        let user = "S-1-5-21-1-2-3-1001";
        let definition = windows_definition(
            label,
            Path::new(r"C:\Tools\llmgw.exe"),
            Path::new(r"C:\config.toml"),
            user,
        );
        let normalized = definition.replace("<Enabled>true</Enabled>", "");
        assert_eq!(
            parse_windows_manager_xml(label, user, &normalized).unwrap(),
            parse_windows_manager_xml(label, user, &definition).unwrap()
        );
        for invalid in ["", "invalid"] {
            let changed = definition.replace(
                "<Enabled>true</Enabled>",
                &format!("<Enabled>{invalid}</Enabled>"),
            );
            assert!(parse_windows_manager_xml(label, user, &changed).is_err());
        }
        let disabled_trigger =
            definition.replacen("<Enabled>true</Enabled>", "<Enabled>false</Enabled>", 1);
        assert!(parse_windows_manager_xml(label, user, &disabled_trigger).is_err());
    }

    #[test]
    fn windows_manager_requires_one_complete_trigger_principal_and_action() {
        let label = "io.llmgw.gateway.shape";
        let target = Path::new(r"C:\Users\fixture\task.xml");
        let user = "S-1-5-21-1-2-3-1001";
        let definition = windows_definition(
            label,
            Path::new(r"C:\Tools\llmgw.exe"),
            Path::new(r"C:\config.toml"),
            user,
        );
        let variants = [
            definition.replace(
                "</Triggers>",
                "<LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>",
            ),
            definition.replace("</Triggers>", "<LogonTrigger/></Triggers>"),
            definition.replace("</Triggers>", "<TimeTrigger/></Triggers>"),
            definition.replace("</Principals>", "<Principal/></Principals>"),
            definition.replace("</Actions>", "<Exec/></Actions>"),
            definition.replace(between(&definition, "<Exec>", "</Exec>").unwrap(), ""),
        ];
        for variant in variants {
            assert!(matches!(
                windows_manager_snapshot_from_xml(label, target, user, &variant),
                ManagerSnapshot::Unknown { .. }
            ));
        }
    }

    #[test]
    fn malformed_windows_manager_xml_uses_a_fixed_safe_reason() {
        let label = "io.llmgw.gateway.malformed";
        let user = "S-1-5-21-1-2-3-1001";
        let definition = windows_definition(
            label,
            Path::new(r"C:\Tools\llmgw.exe"),
            Path::new(r"C:\config.toml"),
            user,
        )
        .replace("</Settings>", "</ForeignElementSentinel>");
        let error = parse_windows_manager_xml(label, user, &definition)
            .unwrap_err()
            .to_string();
        assert_eq!(error, "Windows task XML is malformed");
        assert!(!error.contains("ForeignElementSentinel"));
    }

    #[test]
    fn windows_manager_requires_one_complete_task_document() {
        let label = "io.llmgw.gateway.document";
        let user = "S-1-5-21-1-2-3-1001";
        let definition = windows_definition(
            label,
            Path::new(r"C:\Tools\llmgw.exe"),
            Path::new(r"C:\config.toml"),
            user,
        );
        let declaration = "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n";
        let without_declaration = definition.strip_prefix(declaration).unwrap();
        let rejected = [
            format!(
                "{definition}<Other xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"/>"
            ),
            format!("{definition}unexpected trailing content"),
            format!("{definition}\u{a0}"),
            format!("unexpected leading content{definition}"),
            format!("{definition}&#32;"),
            format!(
                "{definition}<Task xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"/>"
            ),
            format!("{without_declaration}{declaration}"),
            format!(" \n{declaration}{without_declaration}"),
        ];
        for document in rejected {
            assert!(
                parse_windows_manager_xml(label, user, &document).is_err(),
                "unexpected content outside the single Task document must be refused"
            );
        }

        for document in [
            definition.replacen(declaration, &format!("{declaration}<!--prolog-->\n"), 1)
                + "\n<!--epilog-->",
            format!(" \n<!--prolog-->\n{without_declaration}\n<!--epilog-->\n"),
        ] {
            assert!(parse_windows_manager_xml(label, user, &document).is_ok());
        }
    }

    #[test]
    fn linux_manager_definition_exposes_global_or_foreign_scope() {
        let target =
            Path::new("/home/fixture/.config/systemd/user/io.llmgw.gateway.fixture.service");
        let observed = linux_manager_snapshot_from_show(
            target,
            "LoadState=loaded\nUnitFileState=enabled\nFragmentPath=/etc/systemd/user/io.llmgw.gateway.fixture.service\n",
        );
        assert!(matches!(
            observed,
            ManagerSnapshot::Present {
                owned: false,
                enabled: true,
                scope: ManagerScope::GlobalOrForeign,
                ..
            }
        ));
    }

    #[test]
    fn linux_manager_refuses_dropins_pending_reload_and_incomplete_snapshots() {
        let target = Path::new("/home/fixture/.config/systemd/user/review.service");
        let clean = format!(
            "LoadState=loaded\nUnitFileState=enabled\nFragmentPath={}\nDropInPaths=\nNeedDaemonReload=no\n",
            target.display()
        );
        assert!(matches!(
            linux_manager_snapshot_from_show(target, &clean),
            ManagerSnapshot::Present { .. }
        ));
        assert!(matches!(
            linux_manager_snapshot_from_show(
                target,
                &clean.replace(
                    "DropInPaths=",
                    "DropInPaths=/etc/systemd/user/review.service.d/override.conf"
                )
            ),
            ManagerSnapshot::Unknown { .. }
        ));
        assert!(matches!(
            linux_manager_snapshot_from_show(
                target,
                &clean.replace("NeedDaemonReload=no", "NeedDaemonReload=yes")
            ),
            ManagerSnapshot::Unknown { .. }
        ));
        assert!(matches!(
            linux_manager_snapshot_from_show(target, &clean.replace("DropInPaths=\n", "")),
            ManagerSnapshot::Unknown { .. }
        ));
        assert!(matches!(
            linux_manager_snapshot_from_show(target, &clean.replace("NeedDaemonReload=no\n", "")),
            ManagerSnapshot::Unknown { .. }
        ));
    }

    #[test]
    fn linux_removal_preview_orders_reload_before_reobservation() {
        let spec = RegistrationSpec::new(
            Platform::Linux,
            Path::new("/home/fixture/llmgw"),
            Path::new("/home/fixture/config.toml"),
        )
        .unwrap();
        let plan = RegistrationPlan {
            spec,
            action: Action::Off,
            target_path: PathBuf::from("/home/fixture/.config/systemd/user/review.service"),
            snapshot: Snapshot::Present {
                digest: "local".to_owned(),
                owned: true,
                registered_executable: Some(PathBuf::from("/home/fixture/llmgw")),
            },
            manager_snapshot: ManagerSnapshot::Present {
                digest: "manager".to_owned(),
                owned: true,
                enabled: true,
                registered_executable: Some(PathBuf::from("/home/fixture/llmgw")),
                scope: ManagerScope::User,
            },
            state: RegistrationState::Registered,
            preview_hash: "fixture".to_owned(),
        };
        let commands = plan.manager_commands();
        assert!(commands[0].contains(" disable "));
        assert!(commands[1].contains("remove the owned user unit"));
        assert!(commands[2].ends_with("daemon-reload"));
        assert!(commands[3].contains("re-observe remaining influence"));
        assert!(!commands.join(" ").contains("--now"));

        let remaining = linux_post_remove_result(ManagerSnapshot::Present {
            digest: "global".to_owned(),
            owned: false,
            enabled: true,
            registered_executable: None,
            scope: ManagerScope::GlobalOrForeign,
        })
        .unwrap_err()
        .to_string();
        assert!(remaining.contains("local user registration was removed"));
        assert!(remaining.contains("global or foreign"));

        let unknown = linux_post_remove_result(ManagerSnapshot::Unknown {
            reason: "fixture ambiguity".to_owned(),
        })
        .unwrap_err()
        .to_string();
        assert!(unknown.contains("local user registration was removed"));
        assert!(unknown.contains("remaining manager state is unknown"));
    }

    #[test]
    fn windows_query_failure_is_unknown_except_file_or_path_not_found_hresult() {
        assert!(matches!(
            windows_query_failure(Some(0x80070002_u32 as i32), "not found"),
            ManagerSnapshot::Absent
        ));
        assert!(matches!(
            windows_query_failure(Some(0x80070003_u32 as i32), "task folder not found"),
            ManagerSnapshot::Absent
        ));
        assert!(matches!(
            windows_query_failure(Some(0x8004130f_u32 as i32), "account information not set"),
            ManagerSnapshot::Unknown { .. }
        ));
        assert!(matches!(
            windows_query_failure(Some(5), "access denied"),
            ManagerSnapshot::Unknown { .. }
        ));
    }

    #[test]
    fn windows_utf16_task_xml_decodes_before_semantic_parsing() {
        let text = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><Task xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"/>";
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode_windows_xml(&bytes).unwrap(), text);
    }

    #[test]
    fn manager_state_blocks_unknown_and_foreign_but_exposes_owned_disabled() {
        let executable = Path::new("/tmp/llmgw");
        let local = Snapshot::Missing;
        assert_eq!(
            registration_state(
                &local,
                &ManagerSnapshot::Unknown {
                    reason: "blocked".into()
                },
                executable,
            ),
            RegistrationState::Blocked
        );
        assert_eq!(
            registration_state(
                &local,
                &ManagerSnapshot::Present {
                    digest: "x".into(),
                    owned: true,
                    enabled: false,
                    registered_executable: Some(executable.into()),
                    scope: ManagerScope::User,
                },
                executable,
            ),
            RegistrationState::Disabled
        );
    }
}
