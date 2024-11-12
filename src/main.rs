use std::{thread::sleep, time::Duration};

use anyhow::Result;
use esp_idf_svc::hal::prelude::Peripherals;
use led::WS2812RMT;
use log::{error, info};
use mqtt::Mqtt;
use rgb::RGB8;
use utils::{map_range, unix_seconds};
use wifi::WiFi;

mod led;
mod mqtt;
mod utils;
mod wifi;

const DEFAULT_BRIGHTNESS: u8 = 5;
const CLR_WIFI_SCAN: RGB8 = RGB8::new(0, 0, 1); // #0000ff
const CLR_MQTT_CONNECTING: RGB8 = RGB8::new(1, 0, 1); // #ff00ff
const CLR_SLEEPING: RGB8 = RGB8::new(1, 1, 0); // #ffff00
const CLR_MQTT_PUBLISHED: RGB8 = RGB8::new(0, 1, 1); // #00ffff
const CLR_ALL_CONNECTED: RGB8 = RGB8::new(0, 1, 0); //  #00ff00
const CLR_WIFI_WEAK_SIGNAL: RGB8 = RGB8::new(1, 1, 1); //  #ffffff
const CLR_FATAL_ERR: RGB8 = RGB8::new(100, 0, 0); // #ff0000

#[toml_cfg::toml_config]
pub struct Config {
    #[default("MySSID")]
    wifi_ssid: &'static str,
    #[default("1234")]
    wifi_password: &'static str,
    #[default("WPA2Personal")]
    wifi_auth_method: &'static str,
    #[default(20)]
    wifi_max_tx_power: i8,
    #[default(-80)]
    wifi_disconnect_rssi: i32,
    #[default(4)]
    wifi_disconnect_seconds: u32,
    #[default(10)]
    wifi_ignore_rssi_seconds: u32,

    #[default("yourpc.local")]
    mqtt_host: &'static str,
    #[default("presence-node-1")]
    mqtt_node: &'static str,
    #[default("you")]
    mqtt_user: &'static str,
    #[default("1234")]
    mqtt_pass: &'static str,
    #[default("homeassistant")]
    mqtt_discovery_prefix: &'static str,
    #[default("ON")]
    mqtt_on_payload: &'static str,
    #[default("OFF")]
    mqtt_off_payload: &'static str,
    #[default(10)]
    mqtt_disconnected_timeout: u64,
    #[default(300)]
    mqtt_reconnect_timeout: u64,
}

struct State<'a> {
    wifi: WiFi,
    mqtt: Mqtt,
    led: WS2812RMT<'a>,
    wifi_connected_time: Option<u32>,
    wifi_disconn_rssi_start: Option<u32>,
}

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    unsafe {
        esp_idf_svc::sys::nvs_flash_init();
    }
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut peripherals = Peripherals::take().unwrap();

    let mut state = State {
        wifi: WiFi::new(&mut peripherals, CONFIG)?,
        mqtt: Mqtt::new(CONFIG)?,
        led: WS2812RMT::new(peripherals.pins.gpio8, peripherals.rmt.channel0)?,
        wifi_connected_time: None,
        wifi_disconn_rssi_start: None,
    };

    WiFi::set_max_tx_power(CONFIG.wifi_max_tx_power);

    loop {
        if let Err(err) = state.tick() {
            error!("Fatal error: {:?}", err);
            state.set_led_with_brightness(CLR_FATAL_ERR, 1);
            sleep(Duration::from_secs(5));
            break Ok(());
        }
    }
}

impl State<'_> {
    fn tick(&mut self) -> Result<()> {
        sleep(Duration::from_secs(1));

        while !self.wifi.is_connected() {
            self.set_led_with_brightness(CLR_WIFI_SCAN, DEFAULT_BRIGHTNESS);
            self.wifi.connect()?;
            self.wifi_connected_time = Some(unix_seconds());
        }

        if !self.mqtt.has_client() {
            self.mqtt.create_client(CONFIG)?;
        }

        if !self.mqtt.was_connected() {
            if self.mqtt.is_connected() {
                match self.mqtt.publish() {
                    Ok(_) => {
                        self.set_led(CLR_MQTT_PUBLISHED);
                    }
                    Err(err) => return Err(err),
                }
            } else {
                self.set_led(CLR_MQTT_CONNECTING);
            }
            return Ok(());
        }

        if !self.mqtt.is_connected() {
            self.disconnect_and_wait()?;
            return Ok(());
        }

        let rssi = self.wifi.esp_wifi.wifi().get_rssi().unwrap_or(i32::MAX);
        info!("RSSI: {}dBm", rssi);

        if rssi > CONFIG.wifi_disconnect_rssi {
            self.wifi_disconn_rssi_start = None;
            self.set_led(CLR_ALL_CONNECTED);
            return Ok(());
        }

        if let Some(connected_time) = self.wifi_connected_time {
            if unix_seconds() - connected_time <= CONFIG.wifi_ignore_rssi_seconds {
                return Ok(());
            }
        }

        self.set_led(CLR_WIFI_WEAK_SIGNAL);

        let weak_signal_start = match self.wifi_disconn_rssi_start {
            Some(start) => start,
            None => {
                let sec = unix_seconds();
                self.wifi_disconn_rssi_start = Some(sec);
                sec
            }
        };
        if unix_seconds() - weak_signal_start > CONFIG.wifi_disconnect_seconds {
            self.wifi_disconn_rssi_start = None;
            self.disconnect_and_wait()?;
        }

        Ok(())
    }

    fn disconnect_and_wait(&mut self) -> Result<()> {
        if self.mqtt.is_connected() {
            self.mqtt.disconnect();
        }
        self.wifi.disconnect()?;
        self.set_led_with_brightness(CLR_SLEEPING, DEFAULT_BRIGHTNESS);
        sleep(Duration::from_secs(CONFIG.mqtt_reconnect_timeout));
        Ok(())
    }

    fn set_led(&mut self, base_color: RGB8) {
        let brightness = if let Ok(rssi) = self.wifi.esp_wifi.wifi().get_rssi() {
            // 1 isn't enough to turn on the lights, and 255 is *way* too bright.
            map_range(rssi as f32, -100.0, -10.0, 2.0, 30.0)
        } else {
            DEFAULT_BRIGHTNESS
        };
        self.set_led_with_brightness(base_color, brightness);
    }

    fn set_led_with_brightness(&mut self, base_color: RGB8, brightness: u8) {
        let color = RGB8::new(
            base_color.r.saturating_mul(brightness),
            base_color.g.saturating_mul(brightness),
            base_color.b.saturating_mul(brightness),
        );
        if let Err(err) = self.led.set_color(color) {
            error!("Failed to set LED color to {}: {}", color, err);
        }
    }
}
