use anyhow::{anyhow, Result};
use clap::Parser;
use log::{info, warn};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::net::IpAddr;
use std::sync::mpsc;
use std::thread;

#[derive(Parser)]
#[command(
    name = "dreamhost-ddns",
    version,
    about = "Updates a DreamHost DNS A record with the current WAN IP"
)]
struct Args {
    #[arg(short, long)]
    verbose: bool,

    #[arg(short, long)]
    config: Option<String>,

    #[arg(long)]
    api_key: Option<String>,

    #[arg(long)]
    record: Option<String>,

    #[arg(long, default_value_t = 300)]
    interval: u64,

    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct Record {
    record: String,

    #[serde(rename = "type")]
    record_type: String,

    value: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    dreamhost_api_key: String,
    dns_record: String,
}

struct DreamhostClient {
    client: Client,
    api_key: String,
}

impl DreamhostClient {
    fn call(&self, params: &[(&str, &str)]) -> Result<serde_json::Value> {
        let mut query = vec![
            ("key", self.api_key.as_str()),
            ("format", "json"),
        ];

        query.extend_from_slice(params);

        let resp: serde_json::Value = self.client
            .get("https://api.dreamhost.com/")
            .query(&query)
            .send()?
            .json()?;

        if resp["result"] != "success" {
            let reason = resp["reason"]
                .as_str()
                .unwrap_or("Unknown DreamHost API error");

            return Err(anyhow!("DreamHost API error: {}", reason));
        }

        Ok(resp)
    }

    fn get_dns_ip(&self, record_name: &str) -> Result<String> {
        let resp = self.call(&[
            ("cmd", "dns-list_records"),
        ])?;

        let records: Vec<Record> = serde_json::from_value(resp["data"].clone())?;

        records
            .into_iter()
            .find(|r| r.record == record_name && r.record_type == "A")
            .map(|r| r.value)
            .ok_or_else(|| anyhow!("DreamHost error: DNS record '{}' not found", record_name))
    }

    fn update_dns(&self, record: &str, old_ip: &str, new_ip: &str) -> Result<()> {
        info!("Adding new DNS record {} -> {}", record, new_ip);

        self.call(&[
            ("cmd", "dns-add_record"),
            ("record", record),
            ("type", "A"),
            ("value", new_ip),
        ])?;

        info!("Removing old DNS record {} -> {}", record, old_ip);

        self.call(&[
            ("cmd", "dns-remove_record"),
            ("record", record),
            ("type", "A"),
            ("value", old_ip),
        ])?;

        Ok(())
    }
}

fn main() -> Result<()> {

    let args = Args::parse();

    if args.verbose {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Info)
            .init();
    } else {
        env_logger::init();
    }

    let config = resolve_config(&args)?;

    let api_key = args.api_key.unwrap_or(config.dreamhost_api_key);
    let record = args.record.unwrap_or(config.dns_record);

    info!("Record: {}", record);
    info!("Check interval: {} seconds", args.interval);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("dreamhost-ddns/1.0")
        .build()?;

    let dh = DreamhostClient {
        client,
        api_key,
    };

    let wan_ip = get_wan_ip(&dh.client)?;
    info!("Detected WAN IP: {}", wan_ip);

    let dns_ip = dh.get_dns_ip(&record)?;
    info!("DNS record IP: {}", dns_ip);

    if wan_ip.to_string() == dns_ip {
        info!("DNS already up-to-date");
        return Ok(());
    }

    warn!("IP mismatch detected.");

    if args.dry_run {
        info!(
            "DRY RUN: Would update DNS record {} from {} to {}",
            record, dns_ip, wan_ip
        );
    } else {
        info!("Updating DNS...");
        dh.update_dns(&record, &dns_ip, &wan_ip.to_string())?;
        info!("DNS updated successfully");
    }


    Ok(())
}

fn resolve_config(args: &Args) -> Result<Config> {

    let mut api_key = args.api_key.clone();
    let mut record = args.record.clone();

    // Environment variables
    if api_key.is_none() {
        api_key = std::env::var("DREAMHOST_API_KEY").ok();
    }

    if record.is_none() {
        record = std::env::var("DNS_RECORD").ok();
    }

    // Explicit config file
    if (api_key.is_none() || record.is_none()) && args.config.is_some() {
        let cfg = load_config(args.config.as_ref().unwrap())?;

        if api_key.is_none() {
            api_key = Some(cfg.dreamhost_api_key);
        }

        if record.is_none() {
            record = Some(cfg.dns_record);
        }
    }

    // Default config.toml
    if (api_key.is_none() || record.is_none()) && std::path::Path::new("config.toml").exists() {
        let cfg = load_config("config.toml")?;

        if api_key.is_none() {
            api_key = Some(cfg.dreamhost_api_key);
        }

        if record.is_none() {
            record = Some(cfg.dns_record);
        }
    }

    let api_key = api_key.ok_or_else(|| anyhow!("Missing DreamHost API key"))?;
    let record = record.ok_or_else(|| anyhow!("Missing DNS record"))?;

    Ok(Config {
        dreamhost_api_key: api_key,
        dns_record: record,
    })
}

fn load_config(path: &str) -> Result<Config> {
    let contents = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&contents)?;
    Ok(config)
}

fn get_wan_ip(client: &Client) -> Result<IpAddr> {
    let services = [
        "https://icanhazip.com",
        "https://api.ipify.org",
        "https://ifconfig.me/ip",
        "https://checkip.amazonaws.com",
    ];

    let (tx, rx) = mpsc::channel();

    for url in services {
        let tx = tx.clone();
        let client = client.clone();
        let url = url.to_string();

        thread::spawn(move || {
            let result = client.get(&url).send()
                .and_then(|r| r.text())
                .ok()
                .and_then(|text| text.trim().parse::<IpAddr>().ok());

            if let Some(ip) = result {
                let _ = tx.send((url, ip));
            }
        });
    }

    drop(tx);

    match rx.recv() {
        Ok((url, ip)) => {
            info!("WAN IP detected via {}: {}", url, ip);
            Ok(ip)
        }
        Err(_) => Err(anyhow!("All WAN IP detection services failed")),
    }
}