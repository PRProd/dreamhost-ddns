# dreamhost-ddns
DDNS client for Dreamhost written in Rust, compiled for multiple platforms
<br><br>

## Prerequisites
A [dreamhost](https://dreamhost.com) API key is required.  To obtain one, follow the instructions listed in [step 2, here.](https://help.dreamhost.com/hc/en-us/articles/4407354972692-Connecting-to-the-DreamHost-API)
<br><br>

## Download & Setup
1. Choose the appropriate architecture inside the [binaries](/binaries) directory and download the contents (both files)
2. Optionally open and edit the config.toml file with your personal credentials and DNS A record
<br><br>

## Usage

### Windows
```
Usage: dreamhost-ddns.exe [OPTIONS]

Options:
  -v, --verbose
      --log-level <LOG_LEVEL>  [possible values: error, warn, info, debug, trace]
  -c, --config <CONFIG>
      --api-key <API_KEY>
      --record <RECORD>
      --dry-run
  -h, --help                   Print help
  -V, --version                Print version
```
<br><br>

### Linux / Others
```
Usage: dreamhost-ddns [OPTIONS]

Options:
  -v, --verbose
      --log-level <LOG_LEVEL>  [possible values: error, warn, info, debug, trace]
  -c, --config <CONFIG>
      --api-key <API_KEY>
      --record <RECORD>
      --dry-run
  -h, --help                   Print help
  -V, --version                Print version
```
You will likely need to make the dreamhost-ddns file executable first:
```bash
chmod +x dreamhost-ddns
```

Note: When setting this up as a cronjob, it is recommended that you use the --config flag in the crontab entry, and specify the FULL path to your config.toml file
<br><br>


## Configuration
Configuration is quite flexible, suitable for any situation

When using a .toml (or config.toml) file, it should be in the following format:
```toml
dreamhost_api_key = "ENTER-YOUR-DREAMHOST-API-KEY-HERE"
dns_record = "ENTER-THE-TARGET-DNS-A-RECORD"
```
If you've placed a file named **config.toml** into the same directory as the executable, you can run the program simply:
```bash
$ ./dreamhost-ddns
```
If you've named the .toml file differently, or placed it in a different direcory, you can execute the program like this:
```bash
$ ./dreamhost-ddns --config /path/to/my/config/myconfig.toml
```
<br><br>
To override a value in your config file, or to bypass the usage of a config file completely, you can pass the required arguments directly.  

In this example, a .toml config file is not required:
```bash
$ ./dreamhost-ddns --api-key 8SIX753OH9 --record jenny.mydomain.com
```

Values passed in the command line will override values from your configuration file.  In this example, your config file is used only for the API Key, but not for the record:
```bash
$ ./dreamhost-ddns --record jenny.mydomain.com
```
<br><br>

## Important Notes
### Dreamhost API Rate limiting
The dreamhost API is limited to 500 calls daily.  This DDNS client makes a minimum of one API call per run, and between four and eight calls (typically four) when updating the DNS record.  When scheduling this to run, please keep this in mind when deciding how often to run.  When reaching this limit, you will see this error message:
```txt
Error: DreamHost API error: rate error: module dns used more than 500 times in 1 day(s)
```

### Crontab recommendation
It is recommended to pass a configuration file parameter when defining the crontab entry even if you are using the default of config.toml. It is also recommended to set a sane yet aggressive scheduling interval.  For example, this would run every 10 minutes:
```cron
*/10 * * * * /opt/dreamhost-ddns --config /opt/config.toml >> /var/log/dreamhost-ddns.log 2>&1
```

<br><br>
## Under the Hood
### TL/DR:
At the highest level, this is the execution flow:
```txt
Detect WAN IP
        │
        ▼
Get current DNS record IP
        │
        ▼
Compare values
        │
        ├─ same → exit
        │
        └─ different
              │
              ▼
        Safely update DNS
```
<br><br>
### Basic execution is as follows:
```txt
main()
│
├─ parse CLI args
│
├─ configure logging
│
├─ resolve configuration
│      ├─ CLI args
│      ├─ env vars
│      ├─ config file
│      └─ default config.toml
│
├─ build HTTP client
│
├─ create DreamhostClient
│
├─ detect WAN IP
│      └─ parallel IP service queries
│
├─ fetch DNS record IP
│      └─ DreamHost API
│
├─ compare WAN vs DNS
│      │
│      ├─ match → exit
│      │
│      └─ mismatch
│            │
│            ├─ dry-run → log only
│            │
│            └─ update_dns()
│
└─ exit
```
<br><br>

### Detecting WAN IP
Multiple services are queried simultaneously to determine your WAN IP.  If your firewall is blocking all of these services, the application will fail:
 - https://icanhazip.com
 - https://api.ipify.org
 - https://ifconfig.me/ip
 - https://checkip.amazonaws.com

This is how those are used
```txt
get_wan_ip()
│
├─ shuffle service list
│
├─ spawn parallel threads
│
├─ query IP services
│
├─ first successful response wins
│
└─ cancel remaining workers
```
<br><br>

### Updating DNS
In the best case scenario, three API calls are made to update the DNS record.  For safety reasons, a verification is done before the old DNS record is removed.  If the verification fails, it is retried up to five times which involves one additional API call each time.
```txt
update_dns()
│
├─ add new DNS record
│
├─ wait for propagation
│
├─ verify new record exists
│      │
│      ├─ retry up to 5 times
│      │
│      └─ fail → abort update
│
└─ remove old DNS record
```
