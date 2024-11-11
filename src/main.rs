use std::{thread::sleep, time::Duration};

use anyhow::Result;
use esp_idf_svc::hal::prelude::Peripherals;
use led::WS2812RMT;
use log::error;
use mqtt::Mqtt;
use rgb::RGB8;
use utils::{map_range, unix_seconds};
use wifi::WiFi;

mod led;
mod mqtt;
mod utils;
mod wifi;

const DEFAULT_BRIGHTNESS: u8 = 5;
const CLR_WIFI_SCAN_MIN_PWR: RGB8 = RGB8::new(0, 0, 1); // #0000ff
const CLR_WIFI_SCAN_MAX_PWR: RGB8 = RGB8::new(0, 1, 1); // #00ffff
const CLR_MQTT_CONNECTING: RGB8 = RGB8::new(1, 0, 1); // #ff00ff
const CLR_SLEEPING: RGB8 = RGB8::new(1, 1, 0); // #ffff00
const CLR_ALL_CONNECTED_MIN_PWR: RGB8 = RGB8::new(0, 1, 0); //  #00ff00
const CLR_ALL_CONNECTED_MAX_PWR: RGB8 = RGB8::new(1, 1, 1); // #ffffff
const CLR_FATAL_ERR: RGB8 = RGB8::new(100, 0, 0); // #ff0000

#[toml_cfg::toml_config]
pub struct Config {
    #[default("MySSID")]
    wifi_ssid: &'static str,
    #[default("1234")]
    wifi_password: &'static str,
    #[default("WPA2Personal")]
    wifi_auth_method: &'static str,
    #[default(5)]
    wifi_starting_tx_power: i8,
    #[default(20)]
    wifi_max_tx_power: i8,
    #[default(300)]
    wifi_increase_tx_power_seconds: u32,
    #[default(i32::MAX)]
    wifi_disconnect_rssi: i32,

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
    #[default(300)]
    mqtt_min_reconnect_seconds: u64,
}

struct State<'a> {
    wifi: WiFi,
    mqtt: Mqtt,
    led: WS2812RMT<'a>,
    power_on_time: u32,
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
        power_on_time: unix_seconds(),
    };

    WiFi::set_max_tx_power(CONFIG.wifi_starting_tx_power);

    loop {
        if let Err(err) = state.tick() {
            error!("Fatal error: {:?}", err);
            state.set_led_with_brightness(CLR_FATAL_ERR, 1);
            break Ok(());
        }
    }
}

impl State<'_> {
    fn tick(&mut self) -> Result<()> {
        sleep(Duration::from_secs(1));

        while !self.wifi.is_connected() {
            if WiFi::is_max_tx_power() {
                self.set_led_with_brightness(CLR_WIFI_SCAN_MAX_PWR, DEFAULT_BRIGHTNESS);
            } else {
                self.set_led_with_brightness(CLR_WIFI_SCAN_MIN_PWR, DEFAULT_BRIGHTNESS);
            }
            if !self.wifi.connect()?
                && unix_seconds() - self.power_on_time > CONFIG.wifi_increase_tx_power_seconds
            {
                WiFi::set_max_tx_power(CONFIG.wifi_max_tx_power);
            }
        }

        if !self.mqtt.has_client() {
            self.mqtt.create_client(CONFIG)?;
        }

        if !self.mqtt.was_connected() {
            if self.mqtt.is_connected() {
                match self.mqtt.publish() {
                    Ok(_) => {
                        self.set_led(CLR_ALL_CONNECTED_MIN_PWR);
                    }
                    Err(err) => return Err(err),
                }
            } else {
                self.set_led(CLR_MQTT_CONNECTING);
            }
        } else if !self.mqtt.is_connected() {
            self.disconnect_and_wait()?;
        } else if WiFi::is_max_tx_power() {
            self.set_led(CLR_ALL_CONNECTED_MAX_PWR)
        } else {
            let rssi = self.wifi.esp_wifi.wifi().get_rssi().unwrap_or(i32::MAX);
            if rssi < CONFIG.wifi_disconnect_rssi {
                self.disconnect_and_wait()?;
            } else {
                self.set_led(CLR_ALL_CONNECTED_MIN_PWR)
            }
        }

        Ok(())
    }

    fn disconnect_and_wait(&mut self) -> Result<()> {
        if self.mqtt.is_connected() {
            self.mqtt.disconnect();
        }
        self.wifi.disconnect()?;
        self.set_led_with_brightness(CLR_SLEEPING, DEFAULT_BRIGHTNESS);
        sleep(Duration::from_secs(CONFIG.mqtt_min_reconnect_seconds));
        Ok(())
    }

    fn set_led(&mut self, base_color: RGB8) {
        let brightness = if let Ok(rssi) = self.wifi.esp_wifi.wifi().get_rssi() {
            // 1 isn't enough to turn on the lights, and 255 is *way* too bright.
            map_range(rssi as f32, -70.0, -20.0, 2.0, 20.0)
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
