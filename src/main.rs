use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use log::{info, warn, debug, trace};
use rand::seq::SliceRandom;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::net::IpAddr;
use std::sync::mpsc;
use std::thread;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex};

#[derive(Parser)]
#[command(
    name = "dreamhost-ddns",
    version,
    about = "Updates a DreamHost DNS A and AAAA record with the current WAN IP"
)]
struct Args {
    #[arg(short, long)]
    verbose: bool,

    #[arg(long, value_enum)]
    log_level: Option<LogLevel>,

    #[arg(short, long)]
    config: Option<String>,

    #[arg(long)]
    api_key: Option<String>,

    #[arg(long)]
    record: Option<String>,

    #[arg(long)]
    dry_run: bool,

    #[arg(long, conflicts_with = "ipv6_only")]
    ipv4_only: bool,

    #[arg(long, conflicts_with = "ipv4_only")]
    ipv6_only: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Deserialize, Clone)]
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
    record_cache: Mutex<Option<Vec<Record>>>,
}

impl From<LogLevel> for log::LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => log::LevelFilter::Error,
            LogLevel::Warn  => log::LevelFilter::Warn,
            LogLevel::Info  => log::LevelFilter::Info,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Trace => log::LevelFilter::Trace,
        }
    }
}

impl DreamhostClient {

    pub fn new(client: Client, api_key: String) -> Self {
        Self {
            client,
            api_key,
            record_cache: Mutex::new(None),
        }
    }

    fn call(&self, params: &[(&str, &str)]) -> Result<serde_json::Value> {

        let mut query = vec![
            ("key", self.api_key.as_str()),
            ("format", "json"),
        ];

        query.extend_from_slice(params);

        let mut request = self.client
            .get("https://api.dreamhost.com/")
            .query(&query)
            .build()?;

        // ensure user-agent is visible in trace logs
        if !request.headers().contains_key(reqwest::header::USER_AGENT) {
            request.headers_mut().insert(
                reqwest::header::USER_AGENT,
                reqwest::header::HeaderValue::from_str(
                    &format!("dreamhost-ddns/{}", env!("CARGO_PKG_VERSION"))
                )?,
            );
        }

        // ---- TRACE REQUEST LOGGING ----
        if log::log_enabled!(log::Level::Trace) {

            let mut url = request.url().to_string();

            // mask API key
            if let Some(start) = url.find("key=") {
                let end = url[start..].find('&').map(|i| start + i).unwrap_or(url.len());
                url.replace_range(start + 4..end, "***");
            }

            trace!("HTTP Request: {} {}", request.method(), url);

            if request.headers().is_empty() {
                trace!("HTTP Request Headers: <none>");
            } else {
                for (name, value) in request.headers() {
                    trace!("HTTP Header: {} = {:?}", name, value);
                }
            }
        }

        // ---- SEND REQUEST ----
        let response = self.client.execute(request)?;

        // ---- TRACE RESPONSE LOGGING ----
        if log::log_enabled!(log::Level::Trace) {

            trace!("HTTP Status: {}", response.status());

            for (name, value) in response.headers() {
                trace!("Response Header: {} = {:?}", name, value);
            }
        }

        let resp: serde_json::Value = response.json()?;

        if log::log_enabled!(log::Level::Trace) {
            trace!("HTTP Response JSON: {:?}", resp);
        }

        // ---- DREAMHOST API ERROR HANDLING ----
        if resp["result"] != "success" {

            let reason = resp["reason"]
                .as_str()
                .unwrap_or("Unknown DreamHost API error");

            return Err(anyhow!("DreamHost API error: {}", reason));
        }

        Ok(resp)
    }

    fn list_records(&self) -> Result<Vec<Record>> {
        // Check cache first
        let mut cache = self.record_cache.lock().unwrap();
        if let Some(records) = cache.as_ref() {
            debug!("Using cached DNS records");
            return Ok(records.clone());
        }

        let resp = self.call(&[("cmd", "dns-list_records")])?;
        let records: Vec<Record> = serde_json::from_value(resp["data"].clone())?;

        *cache = Some(records.clone()); // store in cache
        Ok(records)
    }

    fn get_dns_ip(&self, record_name: &str, record_type: &str) -> Result<String> {
        let records = self.list_records()?; // uses cache if available

        debug!("All DNS records: {:?}", records);
        records
            .into_iter()
            .find(|r| r.record == record_name && r.record_type == record_type)
            .map(|r| r.value)
            .ok_or_else(|| anyhow!("DreamHost error: {} record '{}' not found", record_type, record_name))
    }

    fn record_exists(&self, record_name: &str, ip: &str, record_type: &str) -> Result<bool> {
        let records = self.list_records()?; // uses cache if available
        Ok(records.iter().any(|r| r.record == record_name && r.record_type == record_type && r.value == ip))
    }

    fn invalidate_cache(&self) {
        let mut cache = self.record_cache.lock().unwrap();
        *cache = None;
    }

    fn update_dns(
        &self,
        record: &str,
        old_ip: &str,
        new_ip: &str,
        record_type: &str
    ) -> Result<()> {

        info!("Adding new {} DNS record {} -> {}", record_type, record, new_ip);
        self.call(&[
            ("cmd", "dns-add_record"),
            ("record", record),
            ("type", record_type),
            ("value", new_ip),
        ])?;

        self.invalidate_cache(); // records have changed, refresh cache

        info!("Waiting briefly for DNS propagation...");
        std::thread::sleep(std::time::Duration::from_secs(3));

        for attempt in 1..=5 {
            if self.record_exists(record, new_ip, record_type)? {
                info!("New {} record verified", record_type);
                break;
            }
            warn!("New {} record not visible yet (attempt {})", record_type, attempt);
            std::thread::sleep(std::time::Duration::from_secs(2));

            if attempt == 5 {
                return Err(anyhow!(
                    "New {} record never appeared; refusing to remove old record",
                    record_type
                ));
            }
        }

        info!("Removing old {} DNS record {} -> {}", record_type, record, old_ip);
        self.call(&[
            ("cmd", "dns-remove_record"),
            ("record", record),
            ("type", record_type),
            ("value", old_ip),
        ])?;

        self.invalidate_cache(); // records changed again

        Ok(())
    }
}

fn check_and_update(
    dh: &DreamhostClient,
    record: &str,
    detected_ip: IpAddr,
    record_type: &str,
    dry_run: bool,
) -> Result<()> {

    match dh.get_dns_ip(record, record_type) {

        Ok(current_ip) => {

            if let Ok(existing_ip) = current_ip.parse::<IpAddr>() {

                if detected_ip == existing_ip {

                    info!("{} record already up-to-date", record_type);
                    return Ok(());

                }

            }

            warn!("{} record mismatch detected", record_type);

            if dry_run {

                info!(
                    "DRY RUN: Would update {} record {} -> {}",
                    record_type,
                    current_ip,
                    detected_ip
                );

                return Ok(());
            }

            dh.update_dns(
                record,
                &current_ip,
                &detected_ip.to_string(),
                record_type,
            )?;

            info!("{} record updated successfully", record_type);

        }

        Err(_) => {

            warn!("{} record does not exist, creating new one", record_type);

            if dry_run {

                info!(
                    "DRY RUN: Would create {} record -> {}",
                    record_type,
                    detected_ip
                );

                return Ok(());
            }

            dh.call(&[
                ("cmd", "dns-add_record"),
                ("record", record),
                ("type", record_type),
                ("value", &detected_ip.to_string()),
            ])?;

            info!("{} record created successfully", record_type);
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let level = if let Some(level) = args.log_level {
        level.into()
    } else if args.verbose {
        log::LevelFilter::Info
    } else {
        log::LevelFilter::Warn
    };

    env_logger::Builder::from_default_env()
        .filter_level(level)
        .init();

    let config = resolve_config(&args)?;

    let api_key = args.api_key.unwrap_or(config.dreamhost_api_key);
    let record = args.record.unwrap_or(config.dns_record);

    info!("Record: {}", record);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent(format!("dreamhost-ddns/{}", env!("CARGO_PKG_VERSION")))
        .build()?;

    let dh = Arc::new(DreamhostClient::new(client.clone(), api_key));

    // ---- Define detection jobs ----
    struct DetectionJob {
        client: Client,
        services: Vec<&'static str>,
        require_ipv4: bool,
        record_type: &'static str,
        record_name: String,
        dry_run: bool,
    }

    impl DetectionJob {
        fn run(self, dh: &Arc<DreamhostClient>) -> Result<()> {
            match detect_ip(&self.client, self.services, self.require_ipv4) {
                Ok(ip) => {
                    info!("Detected {} WAN: {}", self.record_type, ip);
                    check_and_update(dh, &self.record_name, ip, self.record_type, self.dry_run)?;
                }
                Err(_) => {
                    if self.record_type == "AAAA" {
                        info!("No IPv6 WAN detected");
                        match dh.get_dns_ip(&self.record_name, "AAAA") {
                            Ok(existing_ip) => {
                                warn!("IPv6 not detected but AAAA record exists: {}", existing_ip);
                                if self.dry_run {
                                    info!("DRY RUN: Would remove stale AAAA record {}", existing_ip);
                                } else {
                                    dh.call(&[
                                        ("cmd", "dns-remove_record"),
                                        ("record", &self.record_name),
                                        ("type", "AAAA"),
                                        ("value", &existing_ip),
                                    ])?;
                                    warn!("Removed stale AAAA record {}", existing_ip);
                                }
                            }
                            Err(_) => debug!("No AAAA record exists; nothing to remove"),
                        }
                    } else {
                        warn!("No IPv4 WAN detected");
                    }
                }
            }
            Ok(())
        }
    }

    // ---- Build jobs according to flags ----
    let mut jobs = Vec::new();

    // If neither flag is set, run both by default
    if !args.ipv6_only {
        jobs.push(DetectionJob {
            client: client.clone(),
            services: ipv4_services(),
            require_ipv4: true,
            record_type: "A",
            record_name: record.clone(),
            dry_run: args.dry_run,
        });
    }

    if !args.ipv4_only {
        jobs.push(DetectionJob {
            client: client.clone(),
            services: ipv6_services(),
            require_ipv4: false,
            record_type: "AAAA",
            record_name: record.clone(),
            dry_run: args.dry_run,
        });
    }

    if jobs.is_empty() {
        return Err(anyhow!("Both --ipv4-only and --ipv6-only flags cannot be used together; nothing to do"));
    }

    let handles: Vec<_> = jobs
        .into_iter()
        .map(|job| {
            let dh_clone = dh.clone();
            thread::spawn(move || job.run(&dh_clone))
        })
        .collect();

    // ---- Join threads and propagate any errors ----
    for handle in handles {
        handle.join().expect("Thread panicked")?;
    }

    Ok(())
}


fn resolve_config(args: &Args) -> Result<Config> {

    let mut api_key = args.api_key.clone();
    let mut record = args.record.clone();

    if api_key.is_none() {
        api_key = std::env::var("DREAMHOST_API_KEY").ok();
    }

    if record.is_none() {
        record = std::env::var("DNS_RECORD").ok();
    }

    if (api_key.is_none() || record.is_none()) && args.config.is_some() {

        let cfg = load_config(args.config.as_ref().unwrap())?;

        if api_key.is_none() {
            api_key = Some(cfg.dreamhost_api_key);
        }

        if record.is_none() {
            record = Some(cfg.dns_record);
        }
    }

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
    Ok(toml::from_str(&contents)?)
}

fn detect_ip(client: &Client, services: Vec<&str>, require_ipv4: bool) -> Result<IpAddr> {

    let mut services = services;
    services.shuffle(&mut rand::thread_rng());

    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));

    for url in services {

        let tx = tx.clone();
        let client = client.clone();
        let cancel = cancel.clone();
        let url = url.to_string();

        thread::spawn(move || {

            if cancel.load(Ordering::Relaxed) {
                return;
            }

            let result = client
                .get(&url)
                .send()
                .and_then(|r| r.text())
                .ok()
                .and_then(|text| text.trim().parse::<IpAddr>().ok());

            if let Some(ip) = result {

                if require_ipv4 && !ip.is_ipv4() {
                    return;
                }

                if !require_ipv4 && !ip.is_ipv6() {
                    return;
                }

                if !cancel.swap(true, Ordering::Relaxed) {
                    let _ = tx.send((url, ip));
                }
            }
        });
    }

    drop(tx);

    match rx.recv() {
        Ok((url, ip)) => {
            info!("WAN IP detected via {}", url);
            Ok(ip)
        }
        Err(_) => Err(anyhow!("All WAN IP detection services failed")),
    }
}

fn ipv4_services() -> Vec<&'static str> {

    vec![
        "https://icanhazip.com",
        "https://api.ipify.org",
        "https://ident.me",
        "https://ifconfig.me/ip",
        "https://checkip.amazonaws.com",
    ]
}

fn ipv6_services() -> Vec<&'static str> {

    vec![
        "https://api64.ipify.org",
        "https://ipv6.icanhazip.com",
        "https://v6.ident.me",
        "https://api-ipv6.ip.sb/ip",
        "https://ifconfig.co/ip",
    ]
}