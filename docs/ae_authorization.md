# Application Entity (AE) Authorization Implementation

This document describes the implementation of AE (Application Entity) authorization for the Prime DICOM server.

## Overview

The AE authorization system allows you to control which DICOM application entities can connect to and store data in your Prime server. This provides an additional layer of security for your DICOM infrastructure.

## Configuration

### Setting up authorized AEs

To configure authorized AEs, add the following sections to your `prime.toml` configuration file:

```toml
# Enable strict AE authorization (recommended for production)
strict_ae_auth = true

# List of authorized AEs
[[authorized_aes]]
ae_title = "ORTHANC_SCP"
description = "Orthanc PACS server"
allowed_ips = ["192.168.1.100", "10.0.0.50"]

[[authorized_aes]]
ae_title = "RADIOLOGY_MODALITY_A"
description = "CT scanner in Radiology Room A"
allowed_ips = ["10.1.10.20"]
```

### Configuration Options

- `strict_ae_auth`: When set to `true` (recommended for production), only AEs explicitly listed in the `authorized_aes` section will be allowed to connect. When set to `false`, all AEs are accepted (not recommended for production).

- Each authorized AE entry can contain:
  - `ae_title` (required): The DICOM AE title of the application entity
  - `description` (optional): A human-readable description of the AE
  - `allowed_ips` (optional): A list of IP addresses from which this AE is allowed to connect

## Implementation Details

### Architecture

The AE authorization system consists of the following components:

1. **AuthorizedQueryProvider**: A wrapper around the StoreHandler that validates incoming requests
2. **Configuration**: Extended to include the `authorized_aes` and `strict_ae_auth` settings
3. **AE Validation**: Checks that incoming DICOM requests are from authorized AEs

### How it Works

1. When the DICOM server starts, it reads the list of authorized AEs from the configuration
2. The `AuthorizedQueryProvider` wraps the actual StoreHandler
3. For incoming C-STORE requests, the provider checks if the calling AE is authorized
4. If the AE is authorized, the request is processed normally
5. If the AE is not authorized, the request is rejected with an error

## Security Considerations

- For production environments, always set `strict_ae_auth = true`
- Use the `allowed_ips` field to restrict connections to known IP addresses
- Regularly review and update your authorized AE list
- Monitor logs for connection attempts from unauthorized AEs

## Example Configuration

See `prime_example.toml` for a complete example configuration with AE authorization.

## Testing

The implementation includes comprehensive tests for AE authorization functionality:

1. **Configuration Tests:** Verify parsing of the configuration with authorized AEs
2. **AE Validation Tests:** Check that AEs are properly validated against the authorized list
3. **Edge Case Tests:** Test scenarios like empty AE lists, special characters, and case sensitivity
4. **Performance Tests:** Test with large numbers of authorized AEs
5. **Integration Tests:** Test the AuthorizedQueryProvider component

Run these tests with:

```bash
cargo test --test ae_auth_tests  # AE authorization specific tests
cargo test config::tests           # Configuration parsing tests
cargo test                         # All tests
```

## Future Enhancements

Potential future improvements include:

1. IP-based validation at the association level
2. TLS/HTTPS certificate validation
3. Authentication tokens or API keys
4. Audit logging of AE connections
5. Real-time configuration updates without restart