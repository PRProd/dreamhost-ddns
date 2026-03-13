# dreamhost-ddns
![Rust](https://img.shields.io/badge/rust-1.71+-orange.svg)
[![License: GPL v3](https://img.shields.io/badge/License-GPL%20v3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://github.com/PRProd/dreamhost-ddns/actions/workflows/rust.yml/badge.svg)](https://github.com/PRProd/dreamhost-ddns/actions/workflows/rust.yml)
<br>

A lightweight Rust CLI tool that updates a DreamHost **DNS A** and **DNA AAAA** record with your current public WAN IP.

This tool is designed for:

* Home servers with dynamic IPs
* Self-hosted services
* Home Assistant add-ons
* Docker environments
* Simple standalone DDNS setups

It detects your current WAN IP and updates the DNS record **only when necessary**, preventing unnecessary API calls and avoiding DNS downtime.

---

## Features

* **IPv4 and IPv6 support** (dual stack)
* Fast public IP detection using multiple services
* Safe DNS updates with propagation validation
* Reduces API calls via DNS record caching
* Smart error handling (including rate‑limit and API faults)
* Structured logging with configurable levels
* Supports config from CLI, environment variables, or file
* Dry‑run mode for verification w/o making changes
* Small, fast Rust binary
  
---

## How It Works

The updater compares your **current WAN IP** with the **DNS record IP** stored at DreamHost.

```
Detect WAN IP
        │
        ▼
Fetch DNS record from DreamHost
        │
        ▼
Compare values
        │
   ┌────┴────┐
   │         │
Match     Mismatch
   │         │
Exit     Safely update DNS
```

---

## Safe DNS Updates

To prevent DNS outages during updates, the tool performs the following sequence:

1. Add the new DNS record
2. Wait briefly for propagation
3. Verify the new record exists
4. Remove the old DNS record

If verification fails, the old record **is not removed**, ensuring your hostname never loses a valid DNS entry.

---

## WAN IP Detection

Your public IP is detected using multiple services in parallel:

**IPv4 services:**
* https://icanhazip.com
* https://api.ipify.org
* https://ident.me
* https://ifconfig.me/ip
* https://checkip.amazonaws.com

**IPv6 services:**
* https://api64.ipify.org
* https://ipv6.icanhazip.com
* https://v6.ident.me
* https://api-ipv6.ip.sb/ip

The first successful response is used, improving reliability and speed.

---

## Quick Start

Create a [DreamHost API key](https://help.dreamhost.com/hc/en-us/articles/4407354972692-Connecting-to-the-DreamHost-API) with **DNS permissions**, then run:

```bash
dreamhost-ddns \
  --api-key YOUR_API_KEY \
  --record home.example.com
```

If your WAN IP differs from the DNS record, the program updates it automatically.

---

## Configuration

Configuration values can be provided in several ways.

### Configuration Priority

Values are resolved in this order:

1. CLI arguments
2. Environment variables
3. Config file specified with `--config`
4. `config.toml` in the current directory

---

### CLI Arguments

Example:

```bash
dreamhost-ddns \
  --api-key YOUR_API_KEY \
  --record home.example.com
```

Available options:

```
--api-key <KEY>        DreamHost API key
--record <HOSTNAME>    DNS record to update
--config <FILE>        Optional config file
--log-level <LEVEL>    Logging level (error, warn, info, debug, trace)
--verbose              Shortcut for info level
--dry-run              Show actions without modifying DNS
--ipv4-only            Only detect & update IPv4 ("A")
--ipv6-only            Only detect & update IPv6 ("AAAA")
```

---

### Environment Variables

You can also configure the tool using environment variables:

```bash
export DREAMHOST_API_KEY=YOUR_API_KEY
export DNS_RECORD=home.example.com

dreamhost-ddns
```

---

### Config File

Example `config.toml`:

```toml
dreamhost_api_key = "YOUR_API_KEY"
dns_record = "home.example.com"
```

Run with:

```bash
dreamhost-ddns --config config.toml
```

If no configuration options are provided, the program will automatically look for `config.toml` in the current directory.

---

## Logging

Logging verbosity can be controlled using `--log-level`.

Available levels:

```
error
warn
info
debug
trace
```

Example:

```bash
dreamhost-ddns --log-level debug
```

Example output:

```
Detected WAN IP: 203.0.113.15
DNS record IP: 198.51.100.10
IP mismatch detected
Updating DNS...
New DNS record verified
Old DNS record removed
DNS updated successfully
```

---

## Dry Run Mode

To see what changes would be made without modifying DNS:

```bash
dreamhost-ddns --dry-run
```

---

## Building

Clone the repository and build using Cargo:

```bash
git clone https://github.com/PRProd/dreamhost-ddns
cd dreamhost-ddns
cargo build --release
```

The compiled binary will be located at:

```
target/release/dreamhost-ddns
```

---

## Home Assistant Add-on

This tool also powers a Home Assistant add-on:

https://github.com/PRProd/home-assistant-addon-dreamhost-ddns

The add-on wraps this binary and allows configuration directly from the Home Assistant UI.

