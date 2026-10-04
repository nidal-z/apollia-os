use std::path::Path;

use super::{is_representable, Manager, ServiceSpec, LAUNCHD_LABEL, SYSTEMD_UNIT, TASK_NAME};
use crate::service::manager::{
    parse_launchctl_print, parse_scheduled_task, parse_systemctl_show, Observed,
};

fn spec(binary: &str) -> ServiceSpec {
    ServiceSpec {
        binary: binary.to_string(),
        log_file: "/home/ada/.apollia/logs/service.log".to_string(),
        path_env: Some("/usr/local/bin:/usr/bin".to_string()),
        home: "/home/ada".to_string(),
        windows_user: Some("WORK\\ada".to_string()),
        definition: "/home/ada/.apollia/service/apollia-os.xml".to_string(),
    }
}

#[test]
fn test_definition_paths_sit_where_each_manager_reads_them() {
    // GIVEN one home, no XDG override, and its data directory
    let home = Path::new("/home/ada");
    let data = Path::new("/home/ada/.apollia");
    // WHEN each manager places its definition
    let launchd = Manager::Launchd.definition_path(home, None, data);
    let systemd = Manager::Systemd.definition_path(home, None, data);
    let task = Manager::TaskScheduler.definition_path(home, None, data);
    // THEN the LaunchAgent, the user unit and the task XML land in the user's tree
    assert_eq!(
        launchd,
        Path::new("/home/ada/Library/LaunchAgents/com.apollia.os.plist")
    );
    assert_eq!(
        systemd,
        Path::new("/home/ada/.config/systemd/user/apollia-os.service")
    );
    assert_eq!(task, Path::new("/home/ada/.apollia/service/apollia-os.xml"));
}

#[test]
fn test_systemd_unit_follows_xdg_config_home() {
    // GIVEN an XDG_CONFIG_HOME outside the home
    let home = Path::new("/home/ada");
    let xdg = Path::new("/srv/cfg");
    // WHEN the unit path is resolved
    let unit = Manager::Systemd.definition_path(home, Some(xdg), Path::new("/x"));
    // THEN systemd's own lookup directory is used
    assert_eq!(unit, Path::new("/srv/cfg/systemd/user/apollia-os.service"));
}

#[test]
fn test_launchd_plist_runs_start_with_the_absolute_binary_at_load() {
    // GIVEN a binary under a path holding a space and an ampersand
    let spec = spec("/Applications/Apollia OS & co/apollia-os");
    // WHEN the LaunchAgent is rendered
    let plist = Manager::Launchd.render(&spec);
    // THEN it names the label, runs `<binary> start` escaped, starts at load,
    // restarts only after a failure, and logs to the service log
    assert!(plist.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(plist.contains(&format!("<string>{LAUNCHD_LABEL}</string>")));
    assert!(plist.contains(
        "<string>/Applications/Apollia OS &amp; co/apollia-os</string>\n        <string>start</string>"
    ));
    assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
    assert!(plist.contains("<key>SuccessfulExit</key>\n        <false/>"));
    assert!(plist.contains("<string>/home/ada/.apollia/logs/service.log</string>"));
    assert!(plist.contains("<key>PATH</key>\n        <string>/usr/local/bin:/usr/bin</string>"));
    // AND nothing in it asks for a system domain
    assert!(!plist.contains("UserName"));
    assert!(!plist.contains("LaunchDaemons"));
}

#[test]
fn test_launchd_plist_gives_home_and_omits_path_when_none_was_captured() {
    // GIVEN no PATH captured
    let mut spec = spec("/usr/local/bin/apollia-os");
    spec.path_env = None;
    // WHEN the LaunchAgent is rendered
    let plist = Manager::Launchd.render(&spec);
    // THEN the environment carries the install home and no PATH
    assert!(plist.contains("<key>HOME</key>\n        <string>/home/ada</string>\n    </dict>"));
    assert!(!plist.contains("<key>PATH</key>"));
    assert!(plist.ends_with("</dict>\n</plist>\n"));
}

#[test]
fn test_systemd_unit_is_a_user_unit_running_start() {
    // GIVEN a binary path with a space, a percent and a dollar sign
    let spec = spec("/opt/Apollia OS/50%/$bin/apollia-os");
    // WHEN the user unit is rendered
    let unit = Manager::Systemd.render(&spec);
    // THEN ExecStart quotes the path and neutralises specifiers and expansion
    assert!(unit.contains("ExecStart=\"/opt/Apollia OS/50%%/$$bin/apollia-os\" start\n"));
    assert!(unit.contains("Restart=on-failure\n"));
    assert!(unit.contains("Environment=\"HOME=/home/ada\"\n"));
    assert!(unit.contains("Environment=\"PATH=/usr/local/bin:/usr/bin\"\n"));
    // AND it is wanted by the user's default target, never a system one
    assert!(unit.contains("[Install]\nWantedBy=default.target\n"));
    assert!(!unit.contains("User="));
    assert!(!unit.contains("multi-user.target"));
    assert_eq!(Manager::Systemd.label(), SYSTEMD_UNIT);
}

#[test]
fn test_windows_task_starts_at_the_users_logon_without_elevation() {
    // GIVEN a binary under Program Files and a domain user
    let spec = spec("C:\\Program Files\\Apollia OS\\apollia-os.exe");
    // WHEN the task XML is rendered
    let xml = Manager::TaskScheduler.render(&spec);
    // THEN it is UTF-16 declared, CRLF terminated, triggered at this user's logon,
    // runs with the least privilege, and execs `<binary> start`
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\n"));
    assert!(!xml.replace("\r\n", "").contains('\n'));
    assert!(xml.contains(
        "<LogonTrigger>\r\n      <Enabled>true</Enabled>\r\n      <UserId>WORK\\ada</UserId>"
    ));
    assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
    assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
    assert!(xml.contains(
        "<Command>C:\\Program Files\\Apollia OS\\apollia-os.exe</Command>\r\n      <Arguments>start</Arguments>"
    ));
    assert!(xml.contains(&format!("<URI>\\{TASK_NAME}</URI>")));
    // AND the description carries the definition path a status recognises
    assert!(xml.contains("(/home/ada/.apollia/service/apollia-os.xml)"));
    assert!(!xml.contains("HighestAvailable"));
}

#[test]
fn test_windows_task_is_encoded_utf16_with_a_bom() {
    // GIVEN a short text
    // WHEN encoded for each manager
    let task = Manager::TaskScheduler.encode("<a/>");
    let unit = Manager::Systemd.encode("<a/>");
    // THEN the task XML is UTF-16LE with its byte-order mark, the others UTF-8
    assert_eq!(&task[..4], &[0xFF, 0xFE, b'<', 0]);
    assert_eq!(task.len(), 2 + 4 * 2);
    assert_eq!(unit, b"<a/>".to_vec());
}

#[test]
fn test_control_characters_are_not_representable() {
    // GIVEN a path with a newline and an ordinary one
    // WHEN checked
    // THEN only the ordinary path can be written into a definition
    assert!(!is_representable("/tmp/a\nb"));
    assert!(is_representable("/Users/ada/Apollia OS/apollia-os"));
}

#[test]
fn test_launchctl_print_matches_only_this_definition() {
    // GIVEN launchctl answers for a running service loaded from our plist
    let ours = "com.apollia.os = {\n\tactive count = 1\n\tpath = /h/Library/LaunchAgents/com.apollia.os.plist\n\tstate = running\n}";
    // WHEN parsed against our plist and against another home's
    let mine = parse_launchctl_print(ours, "/h/Library/LaunchAgents/com.apollia.os.plist");
    let other = parse_launchctl_print(ours, "/tmp/seed/Library/LaunchAgents/com.apollia.os.plist");
    // THEN only our own definition reads as loaded and running
    assert_eq!(
        mine,
        Observed {
            loaded: true,
            running: true
        }
    );
    assert_eq!(other, Observed::default());
}

#[test]
fn test_launchctl_print_reads_a_stopped_service() {
    // GIVEN a loaded service whose process exited
    let out = "\tpath = /h/p.plist\n\tstate = not running\n";
    // WHEN parsed
    let seen = parse_launchctl_print(out, "/h/p.plist");
    // THEN it is loaded and stopped
    assert!(seen.loaded);
    assert!(!seen.running);
}

#[test]
fn test_systemctl_show_reads_fragment_and_state() {
    // GIVEN systemctl answers for an active unit and for an absent one
    let active = "FragmentPath=/h/.config/systemd/user/apollia-os.service\nActiveState=active\n";
    let absent = "FragmentPath=\nActiveState=inactive\n";
    // WHEN parsed against our unit path
    let unit = "/h/.config/systemd/user/apollia-os.service";
    // THEN the first is ours and running, the second nothing
    assert_eq!(
        parse_systemctl_show(active, unit),
        Observed {
            loaded: true,
            running: true
        }
    );
    assert_eq!(parse_systemctl_show(absent, unit), Observed::default());
}

#[test]
fn test_scheduled_task_reads_state_and_recognises_its_description() {
    // GIVEN Get-ScheduledTask answers a running task whose description names our XML
    let out =
        "Running\r\nApollia OS runtime (C:\\Users\\ada\\.apollia\\service\\apollia-os.xml)\r\n";
    // WHEN parsed against our XML and against another one
    let mine = parse_scheduled_task(out, "C:\\Users\\ada\\.apollia\\service\\apollia-os.xml");
    let other = parse_scheduled_task(out, "C:\\Temp\\seed\\.apollia\\service\\apollia-os.xml");
    // THEN only ours is loaded and running
    assert!(mine.loaded && mine.running);
    assert_eq!(other, Observed::default());
}
