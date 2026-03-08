use anyhow::{anyhow, Result};
use clap::Parser;
use log::{info, warn};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::net::IpAddr;


#[derive(Parser)]
#[command(
    name = "dreamhost-ddns",
    version,
    about = "Updates a DreamHost DNS A record with the current WAN IP",
    long_about = None
)]
struct Args {
    #[arg(short, long)]
    verbose: bool,

    #[arg(short, long, default_value = "config.toml")]
    config: String,

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
struct ApiResponse {
    data: Option<Vec<Record>>,
}

#[derive(Debug, Deserialize)]
struct Config {
    dreamhost_api_key: String,
    dns_record: String,
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

    let config = load_config(&args.config)?;
    let api_key = config.dreamhost_api_key;
    let record = config.dns_record;

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .user_agent("dreamhost-ddns/1.0")
        .build()?;

    let wan_ip = get_wan_ip(&client)?;
    info!("Detected WAN IP: {}", wan_ip);

    let dns_ip = get_dns_ip(&client, &api_key, &record)?;
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
        update_dns(&client, &api_key, &record, &dns_ip, &wan_ip.to_string())?;
    }

    info!("DNS updated successfully");

    Ok(())
}


fn load_config(path: &str) -> Result<Config> {
    let contents = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&contents)?;
    Ok(config)
}


fn get_wan_ip(client: &Client) -> Result<IpAddr> {

    let services = [
        "https://api.ipify.org",
        "https://ifconfig.me/ip",
        "https://checkip.amazonaws.com",
    ];

    for url in services {
        match client.get(url).send() {
            Ok(resp) => {
                if let Ok(text) = resp.text() {
                    let trimmed = text.trim();

                    if let Ok(ip) = trimmed.parse::<IpAddr>() {
                        info!("WAN IP detected via {}: {}", url, ip);
                        return Ok(ip);
                    }
                }
            }
            Err(e) => {
                info!("WAN IP service {} failed: {}", url, e);
                continue;
            }
        }
    }

    Err(anyhow!("Could not determine WAN IP"))
}

fn get_dns_ip(client: &Client, api_key: &str, record_name: &str) -> Result<String> {

    let res: ApiResponse = client
        .get("https://api.dreamhost.com/")
        .query(&[
            ("key", api_key),
            ("cmd", "dns-list_records"),
            ("format", "json"),
        ])
        .send()?
        .json()?;

    let records = res.data.ok_or_else(|| anyhow!("No DNS data returned"))?;

    records
        .into_iter()
        .find(|r| r.record == record_name && r.record_type == "A")
        .map(|r| r.value)
        .ok_or_else(|| anyhow!("DNS record not found"))
}

fn update_dns(client: &Client, api_key: &str, record: &str, old_ip: &str, new_ip: &str) -> Result<()> {

    info!("Adding new DNS record {} -> {}", record, new_ip);

    client
        .get("https://api.dreamhost.com/")
        .query(&[
            ("key", api_key),
            ("cmd", "dns-add_record"),
            ("record", record),
            ("type", "A"),
            ("value", new_ip),
            ("format", "json"),
        ])
        .send()?
        .error_for_status()?;

    info!("New record added successfully");

    info!("Removing old DNS record {} -> {}", record, old_ip);

    client
        .get("https://api.dreamhost.com/")
        .query(&[
            ("key", api_key),
            ("cmd", "dns-remove_record"),
            ("record", record),
            ("type", "A"),
            ("value", old_ip),
            ("format", "json"),
        ])
        .send()?
        .error_for_status()?;

    info!("Old record removed");

    Ok(())
}