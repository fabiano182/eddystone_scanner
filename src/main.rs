use bluer::{Adapter, AdapterEvent, Address,  DiscoveryFilter};
use futures::{pin_mut, StreamExt};
use std::{collections::HashSet, env};
use paho_mqtt as mqtt;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct DeviceInfo {
    name: String,
    address_type: String,
    uuids: Vec<String>,
    paired: bool,
    connected: bool,
    trusted: bool,
    blocked: bool,
    alias: String,
    legacy_pairing: bool,
    rssi: i32,
    manufacturer_data: serde_json::Value,
    service_data: serde_json::Value,
    services_resolved: bool,
}

// async fn query_device(adapter: &Adapter, addr: Address) -> bluer::Result<()> {
//     let device = adapter.device(addr)?;
//     println!("    Address type:       {}", device.address_type().await?);
//     println!("    Name:               {:?}", device.name().await?);
//     println!("    Icon:               {:?}", device.icon().await?);
//     println!("    Class:              {:?}", device.class().await?);
//     println!("    UUIDs:              {:?}", device.uuids().await?.unwrap_or_default());
//     println!("    Paired:             {:?}", device.is_paired().await?);
//     println!("    Connected:          {:?}", device.is_connected().await?);
//     println!("    Trusted:            {:?}", device.is_trusted().await?);
//     println!("    Modalias:           {:?}", device.modalias().await?);
//     println!("    RSSI:               {:?}", device.rssi().await?);
//     println!("    TX power:           {:?}", device.tx_power().await?);
//     println!("    Manufacturer data:  {:?}", device.manufacturer_data().await?);
//     println!("    Service data:       {:?}", device.service_data().await?);
//     Ok(())
// }

async fn query_all_device_properties(adapter: &Adapter, addr: Address) -> bluer::Result<()> {
    let device = adapter.device(addr)?;
    let props = device.all_properties().await?;
    for prop in props {
        println!("    {:?}", &prop);
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    env_logger::init();

    // MQTT Config
    let host = env::var("MQTT_HOST").unwrap_or("localhost".to_string());
    let port = env::var("MQTT_PORT").unwrap_or("1883".to_string());
    let username = env::var("MQTT_USERNAME").unwrap_or("".to_string());
    let password = env::var("MQTT_PASSWORD").unwrap_or("".to_string());

    let create_options = mqtt::CreateOptionsBuilder::new()
        .server_uri(&format!("{}:{}", host, port))
        .client_id("rust-mqtt")
        .finalize();

    let connect_options = mqtt::ConnectOptionsBuilder::new()
        .user_name(&username)
        .password(&password)
        .finalize();

    let cli = mqtt::Client::new(create_options).unwrap_or_else(|e| {
        println!("Failed to create MQTT client: {}", e);
        std::process::exit(1);
    });

    if let Err(e) = cli.connect(connect_options) {
        println!("Failed to connect to MQTT server: {}", e);
        return Ok(());
    }

    // BLE Config
    let filter_addr: HashSet<Address> = env::args().filter_map(|arg| arg.parse::<Address>().ok()).collect();

    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    println!("Discovering devices using Bluetooth adapter {}\n", adapter.name());
    adapter.set_powered(true).await?;

    let filter = DiscoveryFilter {
        ..Default::default()
    };
    adapter.set_discovery_filter(filter).await?;
    println!("Using discovery filter:\n{:#?}\n\n", adapter.discovery_filter().await);
    println!("Filtering devices:\n{:?}\n\n", filter_addr);

    let devices_events = adapter.discover_devices().await?;
    pin_mut!(devices_events);

    loop {
        tokio::select! {
            Some(device_event) = devices_events.next() => {
                match device_event {
                    AdapterEvent::DeviceAdded(addr) => {
                        if !filter_addr.is_empty() && !filter_addr.contains(&addr) {
                            continue;
                        }

                        println!("New device found: {}", addr);
                        query_all_device_properties(&adapter, addr).await?;

                        let payload = adapter.device(addr)?.all_properties().await?;

                        //parse payload to object
                        let json_payload = serde_json::to_string(&payload).unwrap();

                        let msg_content = format!("{:?}", json_payload);
                        let msg_topic = "new_device";
                        let msg = mqtt::Message::new(msg_topic, msg_content.as_bytes(), 1);

                        let _ = cli.publish(msg);
                    }

                    AdapterEvent::DeviceRemoved(addr) => {
                        if !filter_addr.is_empty() && !filter_addr.contains(&addr) {
                            continue;
                        }

                        println!("Device removed: {}", addr);

                        let payload = adapter.device(addr)?.all_properties().await?;
                        let json_payload = serde_json::to_string(&payload).unwrap();

                        let msg_content = format!("{:?}", json_payload);
                        let msg_topic = "device_removed";
                        let msg = mqtt::Message::new(msg_topic, msg_content.as_bytes(), 1);

                        let _ = cli.publish(msg);
                    }

                    _ => (),
                }
                println!();
            }
            else => break
        }
    }
    Ok(())
}