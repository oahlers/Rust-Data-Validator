# Saether Product Data Validator

Rust/Axum API for validating product master data.

## Endpoints
- `GET /health`
- `GET /openapi.json`
- `POST /api/v1/validate-product`

## Run in GitHub Codespaces
```bash
cargo run
```

The app listens on `PORT`, defaulting to `10000`.

## Test
```bash
curl -X POST http://localhost:10000/api/v1/validate-product \
  -H "Content-Type: application/json" \
  -d '{
    "sku": "TEST-001",
    "product_name": "Test Moisturizer",
    "net_weight": 500,
    "gross_weight": 550,
    "weight_unit": "g",
    "width": 10,
    "depth": 5,
    "height": 15,
    "dimension_unit": "cm",
    "units_per_carton": 6,
    "carton_weight": 3500,
    "carton_weight_unit": "g"
  }'
```

## Render settings
- Runtime: Rust
- Build command: `cargo build --release`
- Start command: `./target/release/product-data-validator`
- Health check: `/health`
