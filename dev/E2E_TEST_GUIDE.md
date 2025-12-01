# End-to-End Test: DICOM to JMIX API Retrieval

## Overview

The comprehensive end-to-end test (`test_e2e_cstore_to_jmix_api_retrieval`) validates the complete workflow of the Prime application:

1. **DICOM Ingestion**: Send DICOM files via DICOM C-STORE protocol
2. **JMIX Creation**: Automatic background processing creates JMIX packages
3. **API Retrieval**: Query and retrieve the created package via HTTP API

This test verifies the entire system works as an integrated unit, from receiving DICOM images to delivering them through the REST API.

## Test Workflow

### Step 1: DICOM C-STORE Transmission
- Locates sample DICOM files
- Sends each file via DICOM C-STORE protocol using `storescu`
- Verifies successful transmission
- **Duration**: ~1 second per file

### Step 2: JMIX Package Creation
- Waits for background scanner to detect and process completed studies
- Study stability check: Files must not change for 10 seconds
- Scanner runs every 1 second
- **Total wait**: 15 seconds to ensure processing completes

### Step 3: API Query
- Queries `/api/jmix` to list available packages
- Verifies at least one package was created
- Extracts package ID and metadata

### Step 4: Package Validation
- Verifies package has required fields:
  - `id` - Package identifier
  - `studyInstanceUid` - DICOM study UID
  - `fileSize` - Package size in bytes
  - `createdAt` - Creation timestamp

### Step 5: Manifest Retrieval
- Fetches package manifest via `/api/jmix/{id}/manifest`
- Verifies manifest is valid JSON
- **Result**: Full JMIX manifest with metadata

### Step 6: Resource Enumeration
- Lists all resources in the package via `/api/jmix/{id}/resources`
- Verifies resources can be enumerated
- **Result**: Array of resource metadata

### Step 7: Package Status Check
- Queries `/api/jmix/{id}/status`
- Verifies package exists and is accessible
- Checks manifest readability
- Checks ZIP file accessibility
- **Expected values**: All true, no issues

### Step 8: Package Download
- Downloads package ZIP via `/api/jmix/{id}`
- Verifies file size > 0
- Validates ZIP format (implicit via successful retrieval)
- **Result**: Binary JMIX ZIP package

### Step 9: Query Filtering
- Tests filtering by `studyInstanceUid`
- Verifies filtered results are returned
- **Result**: Confirms filtering works correctly

## Running the Test

### Full end-to-end test only:
```bash
cargo test --test cstore_integration test_e2e_cstore_to_jmix_api_retrieval -- --nocapture
```

### With detailed output:
```bash
RUST_BACKTRACE=1 cargo test --test cstore_integration test_e2e_cstore_to_jmix_api_retrieval -- --nocapture
```

### Run all integration tests:
```bash
cargo test --test cstore_integration -- --nocapture
```

## Expected Output

```
[E2E] Found 15 DICOM files
[E2E] Step 1: Sending DICOM files via C-STORE...
[E2E] ✓ All DICOM files sent successfully
[E2E] Step 2: Waiting for JMIX package creation...
[E2E] Step 3: Querying API for JMIX packages...
[E2E] API returned 1 package(s)
[E2E] Step 4: Retrieving and validating first package...
[E2E] Package ID: <UUID>
[E2E] ✓ Package metadata looks good
[E2E] Step 5: Retrieving package manifest...
[E2E] ✓ Manifest retrieved successfully
[E2E] Step 6: Listing resources in package...
[E2E] ✓ Found N resources
[E2E] Step 7: Checking package status...
[E2E] ✓ Package status: all checks passed
[E2E] Step 8: Downloading package ZIP...
[E2E] ✓ Downloaded package: XXXX bytes
[E2E] Step 9: Testing API filters...
[E2E] ✓ Filters working correctly
[E2E] ✅ Complete end-to-end test PASSED!
```

## Requirements

### System Requirements
- DCMTK tools (`storescu`) must be installed and in PATH
- Python (optional, for DICOM test data generation)
- Sample DICOM files at `samples/study_1/` (included in repo)

### Runtime Requirements
- Minimum 15 seconds (includes study stability check)
- ~2 TCP ports are allocated dynamically for DICOM SCP and HTTP API
- Temporary disk space for test output (~1-50 MB depending on DICOM files)

## Architecture

The test uses a comprehensive `PrimeTestHarness` that:

1. **Allocates ephemeral ports** - Dynamically finds available TCP ports
2. **Creates isolated test directory** - Test files don't interfere with each other
3. **Spawns all services**:
   - DICOM SCP (listening on allocated port)
   - HTTP API (listening on allocated port)
   - Background scanner (runs every 1 second in tests)
4. **Waits for readiness** - Ensures both services are ready before sending DICOM
5. **Cleans up gracefully** - Cancels all tasks on completion

## Test Isolation

Each test run:
- Uses unique TCP ports (no conflicts between parallel tests)
- Has isolated storage directory (`test_output/{test_name}/`)
- Creates independent database instance
- Cleans up after completion (directory preserved for debugging)

## Debugging

### View test output directory:
```bash
ls -la test_output/e2e_cstore_jmix_api/storage/
```

### Check JMIX package contents:
```bash
ls -la test_output/e2e_cstore_jmix_api/storage/jmix/*/
unzip -l test_output/e2e_cstore_jmix_api/storage/jmix/*/*.zip
```

### Enable detailed logging:
```bash
RUST_LOG=debug cargo test --test cstore_integration test_e2e_cstore_to_jmix_api_retrieval -- --nocapture
```

## API Endpoints Validated

| Endpoint | Method | Purpose | Status |
|----------|--------|---------|--------|
| `/api/jmix` | GET | List packages | ✅ Tested |
| `/api/jmix/{id}` | GET | Download package | ✅ Tested |
| `/api/jmix/{id}/manifest` | GET | Get manifest | ✅ Tested |
| `/api/jmix/{id}/resources` | GET | List resources | ✅ Tested |
| `/api/jmix/{id}/status` | GET | Check status | ✅ Tested |
| Query filters | - | Patient/date/modality | ✅ Tested |

## Troubleshooting

### Test times out (> 30 seconds)
- JMIX creation may be slow on system
- Consider increasing the wait time from 15s to 20-25s

### "storescu not available"
- Install DCMTK: `brew install dcmtk` (macOS) or `apt-get install dcmtk` (Linux)
- Or skip the test if DCMTK not needed

### No packages created
- Check if study stability check passed (need 10 seconds with no changes)
- Verify background scanner has enough time to run
- Check test output logs for errors

### Port binding errors
- Ports may still be in TIME_WAIT state from previous test
- Run test again, ports cycle through different ranges
- Or wait 60 seconds and retry

## Performance

- **Average test duration**: 16 seconds (mostly from study stability check)
- **Expected JMIX package size**: 20-50 KB for sample data
- **API response times**: < 100ms per request
- **Memory usage**: ~50-100 MB for test harness

## Success Criteria

The test passes when:
✅ All DICOM files sent successfully
✅ JMIX package created and indexed
✅ Package retrievable via API list
✅ Package metadata is complete
✅ Manifest is accessible and valid
✅ Resources can be enumerated
✅ Package status shows healthy
✅ ZIP file is downloadable and not empty
✅ Query filters work correctly

## Related Tests

- **`test_cstore_to_jmix`**: Basic C-STORE to JMIX conversion (no API)
- **`test_http_health_endpoint`**: HTTP API health check only
- **`test_dicom_scp_accepts_connection`**: DICOM SCP connectivity only
- **API Smoke Tests**: Individual endpoint testing without end-to-end flow
