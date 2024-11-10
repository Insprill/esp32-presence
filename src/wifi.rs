use anyhow::{bail, Result};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{modem::Modem, peripheral::Peripheral, prelude::Peripherals},
    sys::{esp_wifi_get_max_tx_power, esp_wifi_set_max_tx_power, ESP_ERR_INVALID_ARG},
    wifi::{
        AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi, PmfConfiguration,
        ScanMethod,
    },
};
use log::{error, info};

use crate::Config;

pub struct WiFi {
    pub esp_wifi: BlockingWifi<EspWifi<'static>>,
    max_tx_power: i8,
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

        let sysloop = EspSystemEventLoop::take()?;

        let modem: Modem;
        unsafe {
            modem = peripherals.modem.clone_unchecked();
        }

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

        Ok(Self {
            esp_wifi: wifi,
            max_tx_power: config.wifi_max_tx_power,
        })
    }

    pub fn is_connected(&self) -> bool {
        self.esp_wifi.is_connected().unwrap_or(false)
    }

    pub fn connect(&mut self) -> Result<()> {
        info!("Connecting...");

        self.esp_wifi.start()?;
        self.esp_wifi.connect()?;

        unsafe {
            let mut power: i8 = 0;
            esp_wifi_get_max_tx_power(&mut power);
            info!("Current WiFi power: {}dBm", power as f32 * 0.25);
            if self.max_tx_power != i8::MIN {
                if esp_wifi_set_max_tx_power(self.max_tx_power * 4) == ESP_ERR_INVALID_ARG {
                    error!("Invalid WiFi power {}dBm {}", self.max_tx_power, power);
                } else {
                    info!("Set WiFi power to {}dBm", self.max_tx_power);
                }
            }
        }

        info!("Connected! Waiting for DHCP lease...");

        self.esp_wifi.wait_netif_up()?;

        let ip_info = self.esp_wifi.wifi().sta_netif().get_ip_info()?;

        info!("DHCP lease acquired: {:?}", ip_info);

        Ok(())
    }
}
