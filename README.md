# Sæther Product Data Validator V2

## Added in V2
- Volume validation against `width × depth × height`
- Configurable volume tolerance, default 2%
- EAN-13 and GTIN-14 check-digit validation
- Cases-per-layer validation using a simple rectangular grid in both full-carton orientations
- Units-per-pallet validation
- Batch endpoint for validating multiple products in one JSON request

## Endpoints
- `GET /health`
- `GET /openapi.json`
- `POST /api/v1/validate-product`
- `POST /api/v2/validate-products`

## Important cases-per-layer limitation
The packing calculation tests two full-carton orientations. It does not calculate mixed-orientation or advanced bin packing. Use the result as a deterministic first-line validation.

## Run
```bash
cargo fmt
cargo build
cargo run
```

## Single-product test
```bash
curl -X POST http://localhost:10000/api/v1/validate-product \
-H "Content-Type: application/json" \
-d @test-v2.json
```
