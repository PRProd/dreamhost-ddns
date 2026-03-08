# dreamhost-ddns
DDNS client for Dreamhost written in Rust, compiled for multiple platforms
<br><br>

## Prerequisites
A [dreamhost](https://dreamhost.com) API key is required.  To obtain one, follow the instructions listed in [step 2, here.](https://help.dreamhost.com/hc/en-us/articles/4407354972692-Connecting-to-the-DreamHost-API)
<br><br>

## Download & Setup
1. Choose the appropriate architecture inside the [binaries](/binaries) directory and download the contents (both files)
3. Open and edit the config.toml file with your personal credentials and DNS A record
<br><br>

## Usage

### Windows
```
Usage: dreamhost-ddns.exe [OPTIONS]

Options:
  -v, --verbose
  -c, --config <CONFIG>  [default: config.toml]
      --dry-run
  -h, --help             Print help
```
### Linux / Others
```
Usage: dreamhost-ddns [OPTIONS]

Options:
  -v, --verbose
  -c, --config <CONFIG>  [default: config.toml]
      --dry-run
  -h, --help             Print help
```
You will likely need to make the dreamhost-ddns file executable first:
```bash
chmod +x dreamhost-ddns
```

Note: When setting this up as a cronjob, it is recommended that you use the --config flag in the crontab entry, and specify the FULL path to your config.toml file
