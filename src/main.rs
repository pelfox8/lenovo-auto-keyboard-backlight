#![windows_subsystem = "windows"]

use std::collections::HashMap;
use wmi::{COMLibrary, Variant, WMIConnection, WMIResult};

use std::process::{Command};
use std::sync::{Arc, Mutex};
use std::{thread};
use std::os::windows::process::CommandExt;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use rdev::{listen, Event, EventType};
use serde::Deserialize;

const LENOVO_CLASS: &str = "LENOVO_LIGHTING_METHOD";
const CREATE_NO_WINDOW: u32 = 0x08000000;
const TIMEOUT: u64 = 30;

fn check_class() -> bool {
    let wmi_con = get_wmi_connection();
    let query = format!("SELECT * FROM {}", LENOVO_CLASS);
    let results: WMIResult<Vec<HashMap<String, Variant>>> = wmi_con.raw_query(query);

    results.is_ok()
}

fn subscribe_on_change_backlight(backlight_enable: &Arc<Mutex<bool>>) {
    #[derive(Deserialize, Debug)]
    #[serde(rename = "LENOVO_LIGHTING_EVENT")]
    struct LenovoLighting {}

    let wmi_con = get_wmi_connection();

    let iterator = wmi_con.notification::<LenovoLighting>().unwrap();

    for _ in iterator {
        let mut enable_lock = backlight_enable.lock().unwrap();
        *enable_lock = get_current_level() != "1";
    }
}

fn get_wmi_connection() -> WMIConnection {
    let com_lib = COMLibrary::new().unwrap();
    WMIConnection::with_namespace_path("root\\WMI", com_lib).unwrap()
}


fn main() {
    if !check_class() {
        eprintln!("Class '{}' not found. Ensure Lenovo WMI drivers are installed.", LENOVO_CLASS);
        std::process::exit(1);
    }

    let arc_backlight_enable = Arc::new(Mutex::new(get_current_level() != "1"));
    let arc_last_time = Arc::new(Mutex::new(SystemTime::now()));

    let clone_arc_backlight_enable = Arc::clone(&arc_backlight_enable);
    let clone_arc_last_time = Arc::clone(&arc_last_time);
    
    thread::spawn(move || {
        listen(move |event| callback(event, &clone_arc_last_time, &clone_arc_backlight_enable))
            .expect("Error listening");
    });
    
    let clone2_arc_backlight_enable = Arc::clone(&arc_backlight_enable);
    
    thread::spawn(move || {
        subscribe_on_change_backlight(&clone2_arc_backlight_enable)
    });

    loop {
        let duration = SystemTime::now().duration_since(*arc_last_time.lock().unwrap()).unwrap().as_secs();
        if duration > TIMEOUT && *arc_backlight_enable.lock().unwrap() {
            set_backlight(1);
            let mut v = arc_backlight_enable.lock().unwrap();
            *v = false;
        }

        sleep(Duration::from_secs(if duration > TIMEOUT { TIMEOUT } else { TIMEOUT - duration }));
    }
}

fn callback(event: Event, last_time: &Arc<Mutex<SystemTime>>, backlight_enable: &Arc<Mutex<bool>>) {
    if let EventType::KeyPress(_) = event.event_type {
        *last_time.lock().unwrap() = event.time;
        let mut enable_lock = backlight_enable.lock().unwrap();
        if !*enable_lock {
            set_backlight(2);
            *enable_lock = true;
        }
    }
}

fn get_current_level() -> String {
    let mut command = get_command(format!(
        "(Get-WmiObject -namespace root\\WMI -class {}).Get_Lighting_Current_Status(0).Current_Brightness_Level",
        LENOVO_CLASS
    ));
    let output = command.output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn get_command(query: String) -> Command {
    let mut command = Command::new("powershell");
    command.arg("-Command");
    command.arg(query);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn set_backlight(level: u8) {
    get_command(format!(
        "(Get-WmiObject -namespace root\\WMI -class {}).Set_Lighting_Current_Status(0,0,{})",
        LENOVO_CLASS, 
        level
    )).spawn().expect("");
}
