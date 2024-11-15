#![windows_subsystem = "windows"]

use std::collections::HashMap;
use wmi::{COMLibrary, Variant, WMIConnection, WMIResult};

use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::{thread};
use std::os::windows::process::CommandExt;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use rdev::{listen, Event, EventType};

const LENOVO_CLASS: &str = "LENOVO_LIGHTING_METHOD";
const CREATE_NO_WINDOW: u32 = 0x08000000;
const TIMEOUT: u64 = 30;

fn check_class() -> bool {
    let com_lib = COMLibrary::new().unwrap();
    let wmi_con = WMIConnection::with_namespace_path("root\\WMI", com_lib).unwrap();
    let query = format!("SELECT * FROM {}", LENOVO_CLASS);
    let results: WMIResult<Vec<HashMap<String, Variant>>> = wmi_con.raw_query(query);

    results.is_ok()
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
    let output = run_ps_command(format!(
        "(Get-WmiObject -namespace root\\WMI -class {}).Get_Lighting_Current_Status(0).Current_Brightness_Level",
        LENOVO_CLASS
    ));
    if output.status.success() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        panic!("Could not get current level");
    }
}

fn run_ps_command(command: String) -> Output {
    Command::new("powershell")
        .arg("-Command")
        .arg(command)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("Failed to execute command")
}

fn set_backlight(level: u8) {
    run_ps_command(format!(
        "(Get-WmiObject -namespace root\\WMI -class {}).Set_Lighting_Current_Status(0,0,{})",
        LENOVO_CLASS, level
    ));
}
