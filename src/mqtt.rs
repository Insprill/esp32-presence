use crate::Config;
use anyhow::Result;
use esp_idf_svc::mqtt::client::{
    EspMqttClient, LwtConfiguration, MqttClientConfiguration, MqttProtocolVersion, QoS,
};
use log::info;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

pub struct MqttConnectionStatus {
    is_connected: bool,
    was_connected: bool,
}

impl MqttConnectionStatus {
    fn was_connected(&mut self) -> bool {
        let was_connected = self.was_connected;
        self.was_connected = self.is_connected;
        was_connected
    }

    fn set_connected(&mut self, connected: bool) {
        self.was_connected = self.is_connected;
        self.is_connected = connected;
    }
}

pub struct Mqtt {
    client: EspMqttClient<'static>,
    topic: String,
    on_payload: &'static str,
    connection_status: Arc<Mutex<MqttConnectionStatus>>,
}

impl Mqtt {
    pub fn new(config: Config) -> Result<Self> {
        let topic = format!(
            "{}/binary_sensor/{}/state",
            config.mqtt_discovery_prefix, config.mqtt_node
        );

        let broker_url = &format!("mqtt://{}", config.mqtt_host);

        let mqtt_config = MqttClientConfiguration {
            protocol_version: Some(if config.mqtt_3_1_1 {
                MqttProtocolVersion::V3_1_1
            } else {
                MqttProtocolVersion::V3_1
            }),
            username: Some(config.mqtt_user),
            password: Some(config.mqtt_pass),
            client_id: Some(config.mqtt_node),
            keep_alive_interval: Some(Duration::from_secs(15)),
            lwt: Some(LwtConfiguration {
                topic: &topic,
                qos: QoS::AtLeastOnce,
                retain: false,
                payload: config.mqtt_off_payload.as_bytes(),
            }),
            ..Default::default()
        };

        let connection_status = Arc::new(Mutex::new(MqttConnectionStatus {
            is_connected: false,
            was_connected: false,
        }));

        let connection_status_clone = connection_status.clone();
        let client = EspMqttClient::new_cb(broker_url, &mqtt_config, move |event| {
            let new_state = match event.payload().to_string() {
                s if s.starts_with("Connected") => true,
                s if s.starts_with("Disconnected") => false,
                _ => return,
            };

            if let Ok(mut status) = connection_status_clone.lock() {
                status.set_connected(new_state);
            }
        })?;

        Ok(Self {
            client,
            topic,
            on_payload: config.mqtt_on_payload,
            connection_status,
        })
    }

    pub fn is_connected(&self) -> bool {
        self.connection_status
            .lock()
            .map(|status| status.is_connected)
            .unwrap_or(false)
    }

    pub fn was_connected(&self) -> bool {
        self.connection_status
            .lock()
            .map(|mut status| status.was_connected())
            .unwrap_or(false)
    }

    pub fn publish(&mut self) -> Result<()> {
        info!("Publishing {} = {}", self.topic, self.on_payload);

        self.client.publish(
            &self.topic,
            QoS::AtLeastOnce,
            false,
            self.on_payload.as_bytes(),
        )?;

        Ok(())
    }
}
