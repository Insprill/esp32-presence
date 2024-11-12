use anyhow::{bail, Result};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{peripheral::Peripheral, prelude::Peripherals},
    sys::{esp_wifi_set_max_tx_power, ESP_ERR_INVALID_ARG, ESP_ERR_TIMEOUT},
    wifi::{
        AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi, PmfConfiguration,
        ScanMethod,
    },
};
use log::{error, info};

use crate::Config;

pub struct WiFi {
    pub esp_wifi: BlockingWifi<EspWifi<'static>>,
}

impl WiFi {
    pub fn new(peripherals: &mut Peripherals, config: Config) -> Result<Self> {
        let auth_method = if config.wifi_password.is_empty() {
            AuthMethod::None
        } else {
            match config.wifi_auth_method {
                "None" => AuthMethod::None,
                "WPA" => AuthMethod::WPA,
                "WPA2Personal" => AuthMethod::WPA2Personal,
                "WPAWPA2Personal" => AuthMethod::WPAWPA2Personal,
                "WPA3Personal" => AuthMethod::WPA3Personal,
                "WPA2WPA3Personal" => AuthMethod::WPA2WPA3Personal,
                _ => {
                    bail!(
                        "Unsupported WiFi authentication method '{}'!",
                        config.wifi_auth_method
                    )
                }
            }
        };

        let modem = unsafe { peripherals.modem.clone_unchecked() };
        let sysloop = EspSystemEventLoop::take()?;

        let mut esp_wifi = EspWifi::new(modem, sysloop.clone(), None)?;
        esp_wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: config.wifi_ssid.try_into().expect("ssid too long"),
            password: config.wifi_password.try_into().expect("password too long"),
            auth_method,
            scan_method: ScanMethod::FastScan,
            pmf_cfg: PmfConfiguration::Capable { required: false },
            ..Default::default()
        }))?;

        let wifi = BlockingWifi::wrap(esp_wifi, sysloop)?;

        Ok(Self { esp_wifi: wifi })
    }

    pub fn is_connected(&self) -> bool {
        self.esp_wifi.is_connected().unwrap_or(false)
    }

    pub fn connect(&mut self) -> Result<bool> {
        info!("Connecting...");

        if !self.esp_wifi.is_started()? {
            self.esp_wifi.start()?;
        }

        if let Err(err) = self.esp_wifi.connect() {
            if err.code() == ESP_ERR_TIMEOUT {
                return Ok(false);
            }
            return Err(err.into());
        }

        info!("Connected! Waiting for DHCP lease...");

        if let Err(err) = self.esp_wifi.wait_netif_up() {
            if err.code() == ESP_ERR_TIMEOUT {
                return Ok(false);
            }
            return Err(err.into());
        }

        Ok(true)
    }

    pub fn disconnect(&mut self) -> Result<()> {
        Ok(self.esp_wifi.disconnect()?)
    }

    pub fn set_max_tx_power(dbm: i8) {
        if unsafe { esp_wifi_set_max_tx_power(dbm * 4) } == ESP_ERR_INVALID_ARG {
            error!("Invalid WiFi power {}dBm", dbm);
        } else {
            info!("Set WiFi power to {}dBm", dbm);
        }
    }
}
