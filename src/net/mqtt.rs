use once_cell::sync::Lazy;
use rumqttc::{Client, Event, MqttOptions, Outgoing, Packet, TlsConfiguration, Transport};
use rustls::internal::pemfile;
use single::Single;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use super::keys;

const HOSTNAME: &str = "storagebox.local";
const PORT: u16 = 8883;

pub struct MqttPacket {
    pub topic: String,
    pub payload: String,
}

const MQTT_USERNAME: &str = "iota";
static MQTT_PASSWORD: Lazy<String> = Lazy::new(|| keys::read_secret("iota.pwd"));

const CA_CERT_PEM: &str = include_str!("../../res/ca_cert.pem");
const CERT_CHAIN_PEM: &str = include_str!("../../res/chain.pem");
static PRIVKEY_PEM: Lazy<String> = Lazy::new(|| keys::read_secret("iota.privkey.pem"));

pub fn connect() -> (Client, Receiver<MqttPacket>) {
    let cert_chain = pemfile::certs(&mut CERT_CHAIN_PEM.to_owned().as_bytes())
        .expect("Couldn't parse `CERT_CHAIN_PEM`");
    let privkey_list = pemfile::rsa_private_keys(&mut PRIVKEY_PEM.to_owned().as_bytes())
        .expect("Couldn't parse `PRIVKEY_PEM`");

    let privkey = privkey_list
        .into_iter()
        .single()
        .expect("multiple privkeys in PEM file");

    let mut tls_cfg = rustls::ClientConfig::new();
    tls_cfg
        .root_store
        .add_pem_file(&mut CA_CERT_PEM.to_owned().as_bytes())
        .expect("Couldn't parse `CA_CERT_PEM`");
    tls_cfg
        .set_single_client_cert(cert_chain, privkey)
        .expect("Couldn't set client auth info");

    // TODO hash pc hostname for name
    let mut opts = MqttOptions::new("iota", HOSTNAME, PORT);
    opts.set_credentials(MQTT_USERNAME, &MQTT_PASSWORD);
    opts.set_keep_alive(5);
    opts.set_transport(Transport::tls_with_config(TlsConfiguration::from(tls_cfg)));

    let (tx, rx): (Sender<MqttPacket>, Receiver<MqttPacket>) = mpsc::channel();

    let (client, mut connection) = Client::new(opts, 10);
    thread::spawn(move || {
        for evt in connection.iter() {
            match evt {
                Err(_) => {
                    drop(tx);
                    break;
                }
                Ok(Event::Outgoing(Outgoing::Disconnect)) => break,
                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    let r = tx.send(MqttPacket {
                        topic: msg.topic,
                        payload: std::str::from_utf8(&msg.payload)
                            .expect("mqtt payload was not valid UTF-8")
                            .to_owned(),
                    });

                    if r.is_err() {
                        break;
                    }
                }
                Ok(_) => {}
            }
        }
    });

    (client, rx)
}
