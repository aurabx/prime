# Test Organization and Coverage

## Overview
The Prime application has comprehensive test coverage across unit tests, integration tests, and API smoke tests.

## Test Files

### 1. Unit Tests (`src/lib.rs`)
**Count:** 15 tests

Tests core functionality of the application:
- **Database Tests** (7 tests)
  - Database creation and management
  - Package recording and retrieval
  - Package filtering by study UID
  - Index cleanup on deletion
  - Pagination handling

- **Storage Tests** (5 tests)
  - Directory creation and management
  - Temporary study directory handling
  - JMIX package path resolution
  - Cleanup operations

- **Configuration Tests** (3 tests)
  - Configuration file parsing
  - Invalid configuration handling
  - Missing section validation

### 2. API Smoke Tests (`tests/api_smoke_tests.rs`)
**Count:** 13 tests

Comprehensive testing of all HTTP API endpoints:
- Health check endpoint
- Empty package listing
- Package listing with query filters (patient ID, name, modality, dates)
- Individual resource endpoints
- Package deletion
- Error responses (404 errors, JSON formatting)
- All endpoints reachability

Tests verify:
- Proper HTTP status codes
- JSON response formatting
- Query parameter handling
- Error response consistency

### 3. AE Authorization Tests (`tests/ae_auth_tests.rs`)
**Count:** 11 tests

DICOM AE (Application Entity) authorization testing:
- AE title case sensitivity
- IP-based authorization
- Multiple AE configuration
- Empty authorization list handling
- Strict/lenient authentication modes
- Query provider authorization

### 4. Integration Tests (`tests/cstore_integration.rs`)
**Count:** 3 tests

End-to-end workflow testing:
- DICOM SCP startup and connectivity
- HTTP health endpoint availability
- Complete DICOM C-STORE to JMIX conversion workflow
- Package creation verification
- Database indexing

## Running Tests

### Run all tests
```bash
cargo test --all
```

### Run specific test suite
```bash
# Unit tests only
cargo test -p prime

# API smoke tests only
cargo test --test api_smoke_tests

# AE authorization tests only
cargo test --test ae_auth_tests

# Integration tests only
cargo test --test cstore_integration
```

### Run with output
```bash
cargo test -- --nocapture
```

### Run specific test by name
```bash
cargo test smoke_list_packages_with_filters
```

## Test Results Summary

| Category | Count | Status |
|----------|-------|--------|
| Unit Tests | 15 | ✅ Passing |
| API Smoke Tests | 13 | ✅ Passing |
| AE Auth Tests | 11 | ✅ Passing |
| Integration Tests | 3 | ✅ Passing |
| **Total** | **42** | **✅ All Passing** |

## Test Coverage

### API Endpoints Tested
- ✅ `GET /health`
- ✅ `GET /api/jmix`
- ✅ `GET /api/jmix/{id}`
- ✅ `HEAD /api/jmix/{id}`
- ✅ `DELETE /api/jmix/{id}`
- ✅ `GET /api/jmix/{id}/manifest`
- ✅ `GET /api/jmix/{id}/status`
- ✅ `GET /api/jmix/{id}/resources`
- ✅ `GET /api/jmix/{id}/resources/{rid}`
- ✅ `GET /api/jmix/{id}/resources/{rid}/data`

### Query Parameters Tested
- ✅ `studyInstanceUid`
- ✅ `patientId`
- ✅ `patientName`
- ✅ `modality`
- ✅ `startDate` / `endDate`
- ✅ `limit` / `offset`

### Error Conditions Tested
- ✅ 404 Not Found responses
- ✅ JSON error formatting
- ✅ Invalid package IDs
- ✅ Invalid resource IDs
- ✅ Missing database records

## CI/CD Integration

All tests pass and can be integrated into CI/CD pipelines:
```bash
cargo test --all --no-fail-fast
```

Exit code: 0 (success) when all tests pass.

## Performance Notes

- Full test suite runs in approximately 17 seconds
- Integration tests are the longest (~16 seconds) due to DICOM C-STORE operations
- API smoke tests run quickly (~0.8 seconds)
- Unit tests run very quickly (~0.35 seconds)

## Future Test Enhancements

- Load testing for concurrent package queries
- Resource stress testing with large JMIX packages
- Performance benchmarks for filtering operations
- Mock DICOM test data generation
- Additional edge case testing
