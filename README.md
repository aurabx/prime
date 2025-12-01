# Prime

Prime is a DICOM C-STORE to JMIX package converter. It receives DICOM images via the standard DICOM C-STORE protocol and automatically creates JMIX packages from them.

## Features

- **DICOM SCP**: Listens for incoming C-STORE requests on a configurable port and AE title
- **JMIX Packaging**: Automatically creates JMIX-compliant packages from received DICOM studies
- **HTTP API**: RESTful API for querying and retrieving JMIX packages
- **Database**: Uses redb for fast package indexing by Study Instance UID
- **Background Processing**: Automatically detects completed studies and triggers JMIX creation

## Configuration

Copy `prime.toml.example` to `prime.toml` and adjust settings:

```toml
[dicom]
bind_addr = "0.0.0.0"
port = 11112
ae_title = "PRIME_SCP"
max_associations = 10

[storage]
base_path = "./data"
jmix_dir = "jmix-packages"
dicom_temp_dir = "dicom-temp"

[jmix]
skip_hashing = false
skip_listing = false

[http]
bind_addr = "0.0.0.0"
port = 8080

[database]
path = "./data/prime.redb"
```

## Usage

```bash
# Run with default config (prime.toml)
cargo run

# Run with custom config path
cargo run -- /path/to/config.toml
```

## HTTP API

### Health Check
```
GET /health
```

### List JMIX Packages
```
GET /api/jmix
GET /api/jmix?studyInstanceUid=1.2.3.4
GET /api/jmix?patientId=12345
GET /api/jmix?startDate=2024-01-01&endDate=2024-01-31
GET /api/jmix?modality=CT
GET /api/jmix?patientName=John%20Doe
GET /api/jmix?limit=10&offset=0
```

### Get JMIX Package (ZIP download)
```
GET /api/jmix/{id}
```

### Check Package Exists
```
HEAD /api/jmix/{id}
```

### Get Package Manifest
```
GET /api/jmix/{id}/manifest
```

### Get Package Status
```
GET /api/jmix/{id}/status
```

### Delete Package
```
DELETE /api/jmix/{id}
```

### List Resources in Package
```
GET /api/jmix/{id}/resources
```

### Get Resource Metadata
```
GET /api/jmix/{id}/resources/{resourceId}
```

### Download Resource Data
```
GET /api/jmix/{id}/resources/{resourceId}/data
```

## Sending DICOM Images

Use any DICOM SCU to send images to Prime:

```bash
# Using DCMTK storescu
storescu -aet MY_SCU -aec PRIME_SCP localhost 11112 image.dcm

# Send an entire directory
storescu -aet MY_SCU -aec PRIME_SCP localhost 11112 +r /path/to/dicom/
```

## Architecture

```
┌─────────────────┐     ┌─────────────────┐
│  DICOM SCU      │────▶│  DICOM SCP      │
│  (storescu)     │     │  (port 11112)   │
└─────────────────┘     └────────┬────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │  Temp Storage   │
                        │  (by Study UID) │
                        └────────┬────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │  JMIX Creator   │
                        │  (Background)   │
                        └────────┬────────┘
                                 │
                    ┌────────────┴────────────┐
                    ▼                         ▼
           ┌─────────────────┐       ┌─────────────────┐
           │  JMIX Packages  │       │  redb Database  │
           │  (ZIP files)    │       │  (Index)        │
           └─────────────────┘       └─────────────────┘
                    │                         │
                    └────────────┬────────────┘
                                 ▼
                        ┌─────────────────┐
                        │  HTTP API       │
                        │  (port 8080)    │
                        └─────────────────┘
```

## Dependencies

Prime reuses components from harmony-proxy:
- `dimse` crate for DICOM SCP implementation
- `jmix-rs` for JMIX package creation

## License

See LICENSE file.
