use std::path::Path;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("MySSID")]
    wifi_ssid: &'static str,
    #[default("1234")]
    wifi_password: &'static str,
    #[default("WPA2Personal")]
    wifi_auth_method: &'static str,
    #[default(i8::MIN)]
    wifi_max_tx_power: i8,

    #[default("yourpc.local")]
    mqtt_host: &'static str,
    #[default("you")]
    mqtt_user: &'static str,
    #[default("1234")]
    mqtt_pass: &'static str,
}

fn main() {
    if !Path::new("cfg.toml").exists() {
        panic!("You need to create a `cfg.toml` file with your Wi-Fi credentials! Use `cfg.toml.example` as a template.");
    }

    let app_config = CONFIG;

    // WiFi
    if app_config.wifi_ssid == "MySSID" || app_config.wifi_password == "1234" {
        panic!("You need to set the Wi-Fi credentials in `cfg.toml`!");
    }
    if app_config.wifi_ssid.is_empty() {
        panic!("Wi-Fi SSID must be set in `cfg.toml`!")
    }
    if app_config.wifi_ssid.len() > 32 {
        panic!("Wi-Fi SSID cannot be more than 32 bytes!");
    }
    if app_config.wifi_password.len() > 64 {
        panic!("Wi-Fi SSID cannot be more than 64 bytes!");
    }
    match app_config.wifi_auth_method {
        "None" | "WPA" | "WPA2Personal" | "WPAWPA2Personal" | "WPA3Personal"
        | "WPA2WPA3Personal" => {}
        _ => {
            panic!(
                "Unsupported WiFi authentication method '{}'!",
                app_config.wifi_auth_method
            );
        }
    };
    if CONFIG.wifi_max_tx_power != i8::MIN {
        // See `esp_wifi_set_max_tx_power`
        if CONFIG.wifi_max_tx_power < 2 || CONFIG.wifi_max_tx_power > 20 {
            panic!("Invalid wifi_max_tx_power! It must be between 2-20 (inclusive).");
        }
    }

    // MQTT
    if app_config.mqtt_host == "yourpc.local"
        || app_config.mqtt_user == "you"
        || app_config.mqtt_pass == "1234"
    {
        panic!("You need to set the MQTT credentials in `cfg.toml`!");
    }

    embuild::espidf::sysenv::output();
}
