use std::{thread::sleep, time::Duration};

use anyhow::Result;
use esp_idf_svc::hal::prelude::Peripherals;
use led::WS2812RMT;
use log::error;
use mqtt::Mqtt;
use rgb::RGB8;
use wifi::WiFi;

mod led;
mod mqtt;
mod wifi;

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
    #[default(true)]
    mqtt_3_1_1: bool,
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
}

const COLOR_WIFI_SEARCHING: RGB8 = RGB8::new(0, 0, 10);
const COLOR_MQTT_SEARCHING: RGB8 = RGB8::new(5, 0, 10);
const COLOR_WIFI_ERROR: RGB8 = RGB8::new(10, 0, 0);
const COLOR_MQTT_ERROR: RGB8 = RGB8::new(10, 0, 3);
const COLOR_CONNECTED: RGB8 = RGB8::new(0, 2, 0);

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    unsafe {
        esp_idf_svc::sys::nvs_flash_init();
    }
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut peripherals = Peripherals::take().unwrap();

    let mut wifi = WiFi::new(&mut peripherals, CONFIG)?;
    let mut mqtt = Mqtt::new(CONFIG)?;
    let mut led = WS2812RMT::new(peripherals.pins.gpio8, peripherals.rmt.channel0)?;

    loop {
        tick_wifi(&mut led, &mut wifi)?;
        tick_mqtt(&mut led, &mut wifi, &mut mqtt)?;
        sleep(Duration::from_secs(1));
    }
}

fn tick_wifi(led: &mut WS2812RMT, wifi: &mut WiFi) -> Result<()> {
    if !wifi.is_connected() {
        led.set_color(COLOR_WIFI_SEARCHING)?;
        if let Err(err) = wifi.connect() {
            error!("{:?}", err);
            led.set_color(COLOR_WIFI_ERROR)?;
            return Ok(());
        };
    } else if let Ok(rssi) = wifi.esp_wifi.wifi().get_rssi() {
        let color = RGB8::new(
            COLOR_CONNECTED.r,
            map_range(rssi as f32, -70.0, -20.0, 2.0, 40.0),
            COLOR_CONNECTED.b,
        );
        led.set_color(color)?;
    } else {
        led.set_color(COLOR_CONNECTED)?;
    }

    Ok(())
}

fn tick_mqtt(led: &mut WS2812RMT, wifi: &mut WiFi, mqtt: &mut Mqtt) -> Result<()> {
    if wifi.is_connected() {
        if !mqtt.was_connected() && mqtt.is_connected() {
            if let Err(err) = mqtt.publish() {
                error!("{:?}", err);
                led.set_color(COLOR_MQTT_ERROR)?;
            }
        } else if !mqtt.is_connected() {
            // WiFi light just got set, let it show for a bit before overriding it
            sleep(Duration::from_millis(100));
            led.set_color(COLOR_MQTT_SEARCHING)?;
        }
    }

    Ok(())
}

fn map_range(x: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> u8 {
    let x = x.clamp(in_min, in_max);
    let mapped = (x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min;
    mapped.clamp(out_min, out_max) as u8
}
