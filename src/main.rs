#![windows_subsystem = "windows"]

use std::collections::HashMap;
use wmi::{COMLibrary, Variant, WMIConnection, WMIResult};

use std::process::{Command};
use std::sync::{Mutex, RwLock};
use std::{thread};
use std::os::windows::process::CommandExt;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use rdev::{listen, Event, Key};
use serde::Deserialize;
use once_cell::sync::Lazy;
use rdev::EventType::KeyPress;

const LENOVO_CLASS: &str = "LENOVO_LIGHTING_METHOD";
const CREATE_NO_WINDOW: u32 = 0x08000000;
const TIMEOUT: u64 = 30;

static LAST_TIME_KEY_PRESS: Lazy<Mutex<SystemTime>> = Lazy::new(|| Mutex::new(SystemTime::now()));
static BACKLIGHT_STATUS: Lazy<RwLock<bool>> = Lazy::new(|| RwLock::new(false));
static BACKLIGHT_LEVEL: Lazy<RwLock<u8>> = Lazy::new(|| RwLock::new(2));

fn main() {
    check_class();

    thread::spawn(|| {
        listen(callback).expect("Error listening");
    });

    thread::spawn(subscribe_on_change_backlight);

    loop {
        handle_backlight_timeout()
    }
}

fn handle_backlight_timeout() {
    let last_time = *LAST_TIME_KEY_PRESS.lock().unwrap();
    let duration = SystemTime::now().duration_since(last_time).unwrap().as_secs();
    if duration > TIMEOUT {
        change_backlight(false);
    }

    sleep(Duration::from_secs(if duration > TIMEOUT { TIMEOUT } else { TIMEOUT - duration }));
}

fn check_class() {
    let wmi_con = get_wmi_connection();
    let query = format!("SELECT * FROM {}", LENOVO_CLASS);
    let results: WMIResult<Vec<HashMap<String, Variant>>> = wmi_con.raw_query(query);

    if results.is_err() {
        std::process::exit(1);
    }
}

fn subscribe_on_change_backlight() {
    #[derive(Deserialize, Debug)]
    #[serde(rename = "LENOVO_LIGHTING_EVENT")]
    struct LenovoLighting {}

    let wmi_con = get_wmi_connection();

    let iterator = wmi_con.notification::<LenovoLighting>().unwrap();

    for _ in iterator {
        let level = get_current_level();

        *BACKLIGHT_LEVEL.write().unwrap() = level;

        *BACKLIGHT_STATUS.write().unwrap() = level != 1;
    }
}

fn get_wmi_connection() -> WMIConnection {
    let com_lib = COMLibrary::new().unwrap();
    WMIConnection::with_namespace_path("root\\WMI", com_lib).unwrap()
}

fn callback(event: Event) {
    let key = match event.event_type { 
        KeyPress(key) => key,
        _ => return,
    };

    match key {
        Key::DownArrow | Key::UpArrow | Key::LeftArrow |
        Key::RightArrow | Key::Alt | Key::Unknown(_) |
        Key::ControlLeft | Key::ControlRight | Key::Escape |
        Key::Space | Key::AltGr => { return; }
        _ => {}
    };

    *LAST_TIME_KEY_PRESS.lock().unwrap() = event.time;

    change_backlight(true);
}

fn get_current_level() -> u8 {
    let mut command = get_command(format!(
        "(Get-WmiObject -namespace root\\WMI -class {}).Get_Lighting_Current_Status(0).Current_Brightness_Level",
        LENOVO_CLASS
    ));
    let output = command.output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().parse::<u8>().unwrap()
}

fn get_command(query: String) -> Command {
    let mut command = Command::new("powershell");
    command.arg("-Command");
    command.arg(query);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn change_backlight(status: bool) {
    let level = *BACKLIGHT_LEVEL.read().unwrap();
    if level < 2 {
        return;
    }

    *BACKLIGHT_STATUS.write().unwrap() = status;

    let command = format!(
        "(Get-WmiObject -namespace root\\WMI -class {}).Set_Lighting_Current_Status(0,0,{})",
        LENOVO_CLASS,
        if status { level } else { 1 }
    );
    
    get_command(command).spawn().expect("");
}
