//! The service definition each OS service manager reads, rendered as text.
//!
//! Three managers, one per supported OS, each run in the user's own session so
//! no step asks for administrator rights:
//!
//! - macOS: a launchd LaunchAgent under `~/Library/LaunchAgents/`;
//! - Linux: a systemd user unit under `$XDG_CONFIG_HOME/systemd/user/`;
//! - Windows: a Task Scheduler task triggered at the user's logon, whose XML
//!   is kept under the data directory and registered with `schtasks`.
//!
//! Rendering is pure so every definition is tested on every OS; handing the
//! file to the manager lives in the sibling `manager` module.

use std::path::{Path, PathBuf};

/// launchd label of the LaunchAgent, in the reverse-DNS form launchd expects.
/// It follows the desktop bundle identifier (`com.apollia.desktop`).
pub const LAUNCHD_LABEL: &str = "com.apollia.os";

/// systemd user unit name, after the binary it runs.
pub const SYSTEMD_UNIT: &str = "apollia-os.service";

/// Task Scheduler task name, after the binary it runs.
pub const TASK_NAME: &str = "apollia-os";

/// Seconds the service manager waits before starting a failed runtime again.
const RESTART_DELAY_SECS: u32 = 30;

/// The OS service manager that keeps the runtime running for one user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    /// macOS launchd, per-user `gui/<uid>` domain.
    Launchd,
    /// systemd user manager (`systemctl --user`).
    Systemd,
    /// Windows Task Scheduler, a task at the user's logon.
    TaskScheduler,
}

/// What a definition needs to know about the machine it is written on.
#[derive(Debug, Clone)]
pub struct ServiceSpec {
    /// Absolute path of the `apollia-os` binary the service runs.
    pub binary: String,
    /// File the runtime's stdout and stderr are appended to (launchd only:
    /// systemd keeps them in the journal, Task Scheduler discards them).
    pub log_file: String,
    /// `PATH` captured at install time, so the tools an agent runs resolve as
    /// they do in the installing shell. A login service otherwise starts with
    /// the manager's minimal `PATH`.
    pub path_env: Option<String>,
    /// Home directory the service was installed from. launchd and systemd
    /// give it as `HOME`, so the runtime serves the data directory that sits
    /// beside the definition, even when `install` ran under another `HOME`.
    pub home: String,
    /// `DOMAIN\user` the Windows logon trigger and principal name.
    pub windows_user: Option<String>,
    /// Absolute path of the definition itself. Written into the Windows task
    /// description, which is how a later `status` recognises its own task.
    pub definition: String,
}

impl Manager {
    /// The manager of the OS this binary was built for, `None` elsewhere.
    #[must_use]
    pub const fn current() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Manager::Launchd)
        } else if cfg!(target_os = "linux") {
            Some(Manager::Systemd)
        } else if cfg!(target_os = "windows") {
            Some(Manager::TaskScheduler)
        } else {
            None
        }
    }

    /// Stable machine name, published in `--json` as `manager`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Manager::Launchd => "launchd",
            Manager::Systemd => "systemd",
            Manager::TaskScheduler => "task-scheduler",
        }
    }

    /// The name the manager knows the service by.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Manager::Launchd => LAUNCHD_LABEL,
            Manager::Systemd => SYSTEMD_UNIT,
            Manager::TaskScheduler => TASK_NAME,
        }
    }

    /// Where the definition is written.
    ///
    /// `config_home` is `$XDG_CONFIG_HOME` when it is set to an absolute path,
    /// which is the rule systemd itself applies; `data_dir` is `~/.apollia`.
    #[must_use]
    pub fn definition_path(
        self,
        home: &Path,
        config_home: Option<&Path>,
        data_dir: &Path,
    ) -> PathBuf {
        match self {
            Manager::Launchd => home
                .join("Library")
                .join("LaunchAgents")
                .join(format!("{LAUNCHD_LABEL}.plist")),
            Manager::Systemd => config_home
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join(".config"))
                .join("systemd")
                .join("user")
                .join(SYSTEMD_UNIT),
            Manager::TaskScheduler => data_dir.join("service").join(format!("{TASK_NAME}.xml")),
        }
    }

    /// The definition text.
    #[must_use]
    pub fn render(self, spec: &ServiceSpec) -> String {
        match self {
            Manager::Launchd => render_launchd(spec),
            Manager::Systemd => render_systemd(spec),
            Manager::TaskScheduler => render_task(spec),
        }
    }

    /// The bytes written to disk. Task Scheduler reads its XML as UTF-16 with
    /// a byte-order mark, the encoding the declaration announces; the two
    /// other managers read UTF-8.
    #[must_use]
    pub fn encode(self, text: &str) -> Vec<u8> {
        match self {
            Manager::TaskScheduler => {
                let mut out = vec![0xFF, 0xFE];
                for unit in text.encode_utf16() {
                    out.extend_from_slice(&unit.to_le_bytes());
                }
                out
            }
            Manager::Launchd | Manager::Systemd => text.as_bytes().to_vec(),
        }
    }
}

/// True when a path can be written into a definition as is: no control
/// character, which none of the three formats can carry inside a value.
#[must_use]
pub fn is_representable(value: &str) -> bool {
    !value.chars().any(char::is_control)
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// One systemd value inside double quotes: `\` and `"` escaped, `%`
/// specifiers and `$` expansion neutralised.
fn systemd_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    format!("\"{escaped}\"")
}

fn render_launchd(spec: &ServiceSpec) -> String {
    let path = spec
        .path_env
        .as_deref()
        .map(|path| {
            format!(
                "        <key>PATH</key>\n        <string>{}</string>\n",
                xml_escape(path)
            )
        })
        .unwrap_or_default();
    let env = format!(
        r#"    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>{home}</string>
{path}    </dict>
"#,
        home = xml_escape(&spec.home),
    );
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>{delay}</integer>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
{env}</dict>
</plist>
"#,
        label = LAUNCHD_LABEL,
        binary = xml_escape(&spec.binary),
        delay = RESTART_DELAY_SECS,
        log = xml_escape(&spec.log_file),
        env = env,
    )
}

fn render_systemd(spec: &ServiceSpec) -> String {
    let mut env = format!(
        "Environment={}\n",
        systemd_quote(&format!("HOME={}", spec.home))
    );
    if let Some(path) = spec.path_env.as_deref() {
        env.push_str(&format!(
            "Environment={}\n",
            systemd_quote(&format!("PATH={path}"))
        ));
    }
    format!(
        r#"[Unit]
Description=Apollia OS runtime

[Service]
Type=simple
ExecStart={binary} start
Restart=on-failure
RestartSec={delay}
{env}
[Install]
WantedBy=default.target
"#,
        binary = systemd_quote(&spec.binary),
        delay = RESTART_DELAY_SECS,
        env = env,
    )
}

/// The task XML, in CRLF line endings as Task Scheduler writes its own.
fn render_task(spec: &ServiceSpec) -> String {
    let user = spec
        .windows_user
        .as_deref()
        .map(|u| format!("<UserId>{}</UserId>", xml_escape(u)))
        .unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Apollia OS runtime ({definition})</Description>
    <URI>\{task}</URI>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      {user}
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      {user}
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>3</Count>
    </RestartOnFailure>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{binary}</Command>
      <Arguments>start</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        definition = xml_escape(&spec.definition),
        task = TASK_NAME,
        user = user,
        binary = xml_escape(&spec.binary),
    )
    .replace('\n', "\r\n")
}

#[cfg(test)]
#[path = "definition_tests.rs"]
mod tests;
