use axum::{
    http::{header, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, net::SocketAddr};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;

#[derive(Debug, Deserialize)]
struct ProductValidationRequest {
    sku: String,
    product_name: Option<String>,
    net_weight: f64,
    gross_weight: f64,
    weight_unit: String,
    width: f64,
    depth: f64,
    height: f64,
    dimension_unit: String,
    units_per_carton: u32,
    carton_weight: f64,
    carton_weight_unit: String,
    pallet: Option<PalletData>,
}

#[derive(Debug, Deserialize)]
struct PalletData {
    cases_per_layer: u32,
    layers_per_pallet: u32,
    carton_height: f64,
    height_unit: String,
    pallet_base_height: f64,
    max_total_height: f64,
}

#[derive(Debug, Serialize)]
struct ProductValidationResponse {
    sku: String,
    product_name: Option<String>,
    overall_status: ValidationStatus,
    summary: ValidationSummary,
    normalized_data: NormalizedProductData,
    pallet_calculation: Option<PalletCalculation>,
    validations: Vec<ValidationResult>,
}

#[derive(Debug, Serialize)]
struct ValidationSummary {
    passed: usize,
    warnings: usize,
    failed: usize,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
enum ValidationStatus {
    Pass,
    Warning,
    Fail,
}

#[derive(Debug, Serialize)]
struct ValidationResult {
    rule: String,
    status: ValidationStatus,
    message: String,
    details: Option<Value>,
}

#[derive(Debug, Serialize)]
struct NormalizedProductData {
    net_weight_g: f64,
    gross_weight_g: f64,
    carton_weight_g: f64,
    width_cm: f64,
    depth_cm: f64,
    height_cm: f64,
    units_per_carton: u32,
    expected_minimum_carton_weight_g: f64,
}

#[derive(Debug, Serialize)]
struct PalletCalculation {
    cases_per_layer: u32,
    layers_per_pallet: u32,
    total_cases_per_pallet: u64,
    total_units_per_pallet: u64,
    carton_height_cm: f64,
    goods_height_cm: f64,
    pallet_base_height_cm: f64,
    total_pallet_height_cm: f64,
    max_total_height_cm: f64,
    remaining_height_cm: f64,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    version: &'static str,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: &'static str,
    message: String,
    field: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "product_data_validator=info,tower_http=info".into()),
        )
        .init();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/openapi.json", get(openapi))
        .route("/api/v1/validate-product", post(validate_product))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "10000".to_string())
        .parse()
        .expect("PORT must be a valid number");

    let address = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Product Data Validator listening on {}", address);

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("Could not bind to address");

    axum::serve(listener, app).await.expect("Server failed");
}

async fn root() -> Json<Value> {
    Json(json!({
        "service": "Saether Product Data Validator",
        "version": "1.1.0",
        "status": "running",
        "endpoints": {
            "health": "/health",
            "validate_product": "/api/v1/validate-product",
            "openapi": "/openapi.json"
        }
    }))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "product-data-validator",
        version: "1.1.0",
    })
}

async fn validate_product(
    Json(product): Json<ProductValidationRequest>,
) -> Result<Json<ProductValidationResponse>, (StatusCode, Json<ApiError>)> {
    validate_positive_value("net_weight", product.net_weight)?;
    validate_positive_value("gross_weight", product.gross_weight)?;
    validate_positive_value("width", product.width)?;
    validate_positive_value("depth", product.depth)?;
    validate_positive_value("height", product.height)?;
    validate_positive_value("carton_weight", product.carton_weight)?;

    if product.units_per_carton == 0 {
        return Err(bad_request(
            "units_per_carton",
            "Units per carton must be greater than zero.",
        ));
    }

    let net_weight_g = convert_weight_to_grams(product.net_weight, &product.weight_unit, "weight_unit")?;
    let gross_weight_g = convert_weight_to_grams(product.gross_weight, &product.weight_unit, "weight_unit")?;
    let carton_weight_g = convert_weight_to_grams(
        product.carton_weight,
        &product.carton_weight_unit,
        "carton_weight_unit",
    )?;

    let width_cm = convert_dimension_to_centimetres(product.width, &product.dimension_unit, "dimension_unit")?;
    let depth_cm = convert_dimension_to_centimetres(product.depth, &product.dimension_unit, "dimension_unit")?;
    let height_cm = convert_dimension_to_centimetres(product.height, &product.dimension_unit, "dimension_unit")?;

    let expected_minimum_carton_weight_g = gross_weight_g * product.units_per_carton as f64;
    let mut validations = Vec::new();

    validations.push(ValidationResult {
        rule: "unit_conversion".to_string(),
        status: ValidationStatus::Pass,
        message: "All supported units were normalized successfully.".to_string(),
        details: Some(json!({
            "weight_unit_standard": "g",
            "dimension_unit_standard": "cm",
            "net_weight_g": round(net_weight_g),
            "gross_weight_g": round(gross_weight_g),
            "carton_weight_g": round(carton_weight_g),
            "width_cm": round(width_cm),
            "depth_cm": round(depth_cm),
            "height_cm": round(height_cm)
        })),
    });

    if net_weight_g > gross_weight_g {
        validations.push(ValidationResult {
            rule: "net_vs_gross_weight".to_string(),
            status: ValidationStatus::Fail,
            message: "Net weight exceeds gross weight.".to_string(),
            details: Some(json!({
                "net_weight_g": round(net_weight_g),
                "gross_weight_g": round(gross_weight_g),
                "difference_g": round(net_weight_g - gross_weight_g)
            })),
        });
    } else {
        validations.push(ValidationResult {
            rule: "net_vs_gross_weight".to_string(),
            status: ValidationStatus::Pass,
            message: "Gross weight is not lower than net weight.".to_string(),
            details: Some(json!({
                "net_weight_g": round(net_weight_g),
                "gross_weight_g": round(gross_weight_g)
            })),
        });
    }

    if carton_weight_g < expected_minimum_carton_weight_g {
        validations.push(ValidationResult {
            rule: "carton_weight_validation".to_string(),
            status: ValidationStatus::Fail,
            message: "Carton weight is lower than the combined gross weight of its units.".to_string(),
            details: Some(json!({
                "units_per_carton": product.units_per_carton,
                "unit_gross_weight_g": round(gross_weight_g),
                "expected_minimum_carton_weight_g": round(expected_minimum_carton_weight_g),
                "actual_carton_weight_g": round(carton_weight_g),
                "shortfall_g": round(expected_minimum_carton_weight_g - carton_weight_g)
            })),
        });
    } else {
        validations.push(ValidationResult {
            rule: "carton_weight_validation".to_string(),
            status: ValidationStatus::Pass,
            message: "Carton weight is not lower than the expected minimum.".to_string(),
            details: Some(json!({
                "units_per_carton": product.units_per_carton,
                "expected_minimum_carton_weight_g": round(expected_minimum_carton_weight_g),
                "actual_carton_weight_g": round(carton_weight_g)
            })),
        });
    }

    if width_cm < depth_cm {
        validations.push(ValidationResult {
            rule: "width_depth_validation".to_string(),
            status: ValidationStatus::Warning,
            message: "Width is smaller than depth. The values may be reversed.".to_string(),
            details: Some(json!({
                "current_width_cm": round(width_cm),
                "current_depth_cm": round(depth_cm),
                "suggested_width_cm": round(depth_cm),
                "suggested_depth_cm": round(width_cm)
            })),
        });
    } else {
        validations.push(ValidationResult {
            rule: "width_depth_validation".to_string(),
            status: ValidationStatus::Pass,
            message: "Width is greater than or equal to depth.".to_string(),
            details: Some(json!({
                "width_cm": round(width_cm),
                "depth_cm": round(depth_cm)
            })),
        });
    }

    let pallet_calculation = if let Some(pallet) = product.pallet {
        if pallet.cases_per_layer == 0 {
            return Err(bad_request("pallet.cases_per_layer", "Cases per layer must be greater than zero."));
        }
        if pallet.layers_per_pallet == 0 {
            return Err(bad_request("pallet.layers_per_pallet", "Layers per pallet must be greater than zero."));
        }
        validate_positive_value("pallet.carton_height", pallet.carton_height)?;
        validate_positive_value("pallet.pallet_base_height", pallet.pallet_base_height)?;
        validate_positive_value("pallet.max_total_height", pallet.max_total_height)?;

        let carton_height_cm = convert_dimension_to_centimetres(
            pallet.carton_height,
            &pallet.height_unit,
            "pallet.height_unit",
        )?;
        let pallet_base_height_cm = convert_dimension_to_centimetres(
            pallet.pallet_base_height,
            &pallet.height_unit,
            "pallet.height_unit",
        )?;
        let max_total_height_cm = convert_dimension_to_centimetres(
            pallet.max_total_height,
            &pallet.height_unit,
            "pallet.height_unit",
        )?;

        let total_cases_per_pallet =
            pallet.cases_per_layer as u64 * pallet.layers_per_pallet as u64;
        let total_units_per_pallet =
            total_cases_per_pallet * product.units_per_carton as u64;
        let goods_height_cm = carton_height_cm * pallet.layers_per_pallet as f64;
        let total_pallet_height_cm = pallet_base_height_cm + goods_height_cm;
        let remaining_height_cm = max_total_height_cm - total_pallet_height_cm;

        let (status, message) = if total_pallet_height_cm > max_total_height_cm {
            (
                ValidationStatus::Fail,
                "Calculated pallet height exceeds the configured maximum total height.",
            )
        } else {
            (
                ValidationStatus::Pass,
                "Calculated pallet height is within the configured maximum total height.",
            )
        };

        validations.push(ValidationResult {
            rule: "pallet_height_validation".to_string(),
            status,
            message: message.to_string(),
            details: Some(json!({
                "cases_per_layer": pallet.cases_per_layer,
                "layers_per_pallet": pallet.layers_per_pallet,
                "total_cases_per_pallet": total_cases_per_pallet,
                "total_units_per_pallet": total_units_per_pallet,
                "carton_height_cm": round(carton_height_cm),
                "goods_height_cm": round(goods_height_cm),
                "pallet_base_height_cm": round(pallet_base_height_cm),
                "total_pallet_height_cm": round(total_pallet_height_cm),
                "max_total_height_cm": round(max_total_height_cm),
                "remaining_height_cm": round(remaining_height_cm)
            })),
        });

        Some(PalletCalculation {
            cases_per_layer: pallet.cases_per_layer,
            layers_per_pallet: pallet.layers_per_pallet,
            total_cases_per_pallet,
            total_units_per_pallet,
            carton_height_cm: round(carton_height_cm),
            goods_height_cm: round(goods_height_cm),
            pallet_base_height_cm: round(pallet_base_height_cm),
            total_pallet_height_cm: round(total_pallet_height_cm),
            max_total_height_cm: round(max_total_height_cm),
            remaining_height_cm: round(remaining_height_cm),
        })
    } else {
        None
    };

    let passed = validations.iter().filter(|r| r.status == ValidationStatus::Pass).count();
    let warnings = validations.iter().filter(|r| r.status == ValidationStatus::Warning).count();
    let failed = validations.iter().filter(|r| r.status == ValidationStatus::Fail).count();

    let overall_status = if failed > 0 {
        ValidationStatus::Fail
    } else if warnings > 0 {
        ValidationStatus::Warning
    } else {
        ValidationStatus::Pass
    };

    Ok(Json(ProductValidationResponse {
        sku: product.sku,
        product_name: product.product_name,
        overall_status,
        summary: ValidationSummary { passed, warnings, failed },
        normalized_data: NormalizedProductData {
            net_weight_g: round(net_weight_g),
            gross_weight_g: round(gross_weight_g),
            carton_weight_g: round(carton_weight_g),
            width_cm: round(width_cm),
            depth_cm: round(depth_cm),
            height_cm: round(height_cm),
            units_per_carton: product.units_per_carton,
            expected_minimum_carton_weight_g: round(expected_minimum_carton_weight_g),
        },
        pallet_calculation,
        validations,
    }))
}

fn validate_positive_value(field: &str, value: f64) -> Result<(), (StatusCode, Json<ApiError>)> {
    if !value.is_finite() || value <= 0.0 {
        return Err(bad_request(field, "The value must be a finite number greater than zero."));
    }
    Ok(())
}

fn convert_weight_to_grams(
    value: f64,
    unit: &str,
    field: &str,
) -> Result<f64, (StatusCode, Json<ApiError>)> {
    match normalize_unit(unit).as_str() {
        "g" | "gram" | "grams" => Ok(value),
        "kg" | "kilogram" | "kilograms" => Ok(value * 1000.0),
        "oz" | "ounce" | "ounces" => Ok(value * 28.349_523_125),
        unsupported => Err(bad_request(
            field,
            &format!("Unsupported weight unit '{}'. Supported units are g, kg, and oz.", unsupported),
        )),
    }
}

fn convert_dimension_to_centimetres(
    value: f64,
    unit: &str,
    field: &str,
) -> Result<f64, (StatusCode, Json<ApiError>)> {
    match normalize_unit(unit).as_str() {
        "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => Ok(value),
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => Ok(value / 10.0),
        "m" | "meter" | "meters" | "metre" | "metres" => Ok(value * 100.0),
        "in" | "inch" | "inches" => Ok(value * 2.54),
        unsupported => Err(bad_request(
            field,
            &format!("Unsupported dimension unit '{}'. Supported units are mm, cm, m, in, and inch.", unsupported),
        )),
    }
}

fn normalize_unit(unit: &str) -> String {
    unit.trim().to_lowercase()
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn bad_request(field: &str, message: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "validation_error",
            message: message.to_string(),
            field: Some(field.to_string()),
        }),
    )
}

async fn openapi() -> impl IntoResponse {
    Json(json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Saether Product Data Validator API",
            "description": "Validates product weights, dimensions, packaging hierarchy, units, and optional pallet height.",
            "version": "1.1.0"
        },
        "paths": {
            "/health": {
                "get": {
                    "operationId": "HealthCheck",
                    "summary": "Check API health",
                    "responses": { "200": { "description": "API is healthy" } }
                }
            },
            "/api/v1/validate-product": {
                "post": {
                    "operationId": "ValidateProduct",
                    "summary": "Validate product and pallet master data",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ProductValidationRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Product validation completed" },
                        "400": { "description": "Invalid input" }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "ProductValidationRequest": {
                    "type": "object",
                    "required": [
                        "sku", "net_weight", "gross_weight", "weight_unit",
                        "width", "depth", "height", "dimension_unit",
                        "units_per_carton", "carton_weight", "carton_weight_unit"
                    ],
                    "properties": {
                        "sku": { "type": "string", "example": "TEST-001" },
                        "product_name": { "type": "string", "nullable": true, "example": "Test Moisturizer" },
                        "net_weight": { "type": "number", "example": 500 },
                        "gross_weight": { "type": "number", "example": 550 },
                        "weight_unit": { "type": "string", "enum": ["g", "kg", "oz"], "example": "g" },
                        "width": { "type": "number", "example": 10 },
                        "depth": { "type": "number", "example": 5 },
                        "height": { "type": "number", "example": 15 },
                        "dimension_unit": { "type": "string", "enum": ["mm", "cm", "m", "in", "inch"], "example": "cm" },
                        "units_per_carton": { "type": "integer", "minimum": 1, "example": 6 },
                        "carton_weight": { "type": "number", "example": 3500 },
                        "carton_weight_unit": { "type": "string", "enum": ["g", "kg", "oz"], "example": "g" },
                        "pallet": { "$ref": "#/components/schemas/PalletData" }
                    }
                },
                "PalletData": {
                    "type": "object",
                    "required": [
                        "cases_per_layer", "layers_per_pallet", "carton_height",
                        "height_unit", "pallet_base_height", "max_total_height"
                    ],
                    "properties": {
                        "cases_per_layer": { "type": "integer", "minimum": 1, "example": 20 },
                        "layers_per_pallet": { "type": "integer", "minimum": 1, "example": 8 },
                        "carton_height": { "type": "number", "example": 18 },
                        "height_unit": { "type": "string", "enum": ["mm", "cm", "m", "in", "inch"], "example": "cm" },
                        "pallet_base_height": { "type": "number", "example": 14.4 },
                        "max_total_height": { "type": "number", "example": 180 }
                    }
                }
            }
        }
    }))
}
