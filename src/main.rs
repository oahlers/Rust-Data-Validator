use axum::{
    http::{header, Method, StatusCode},
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

const VERSION: &str = "2.6.3";

#[derive(Debug, Deserialize, Clone)]
struct ProductValidationRequest {
    sku: String,
    product_name: Option<String>,
    ean13: Option<String>,
    gtin14: Option<String>,
    net_weight: f64,
    gross_weight: f64,
    weight_unit: String,
    width: f64,
    depth: f64,
    height: f64,
    dimension_unit: String,
    declared_volume: Option<f64>,
    volume_unit: Option<String>,
    volume_tolerance_percent: Option<f64>,
    units_per_carton: u32,
    carton_weight: f64,
    carton_weight_unit: String,
    pallet: Option<PalletData>,
}

#[derive(Debug, Deserialize)]
struct ProductJsonRequest {
    product_json: String,
}

#[derive(Debug, Deserialize, Clone)]
struct PalletData {
    cases_per_layer: Option<u32>,
    layers_per_pallet: Option<u32>,
    carton_width: f64,
    carton_depth: f64,
    carton_height: f64,
    pallet_width: f64,
    pallet_depth: f64,
    height_unit: String,
    pallet_base_height: f64,
    max_total_height: f64,
    declared_units_per_pallet: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct BatchValidationRequest {
    products: Vec<ProductValidationRequest>,
}

#[derive(Debug, Serialize)]
struct BatchValidationResponse {
    summary: BatchSummary,
    products: Vec<ProductValidationResponse>,
}

#[derive(Debug, Serialize)]
struct BatchSummary {
    total_products: usize,
    passed: usize,
    warnings: usize,
    failed: usize,
}

#[derive(Debug, Serialize)]
struct ProductValidationResponse {
    sku: String,
    product_name: Option<String>,
    overall_status: ValidationStatus,
    summary: ValidationSummary,
    validation_statuses: ValidationStatuses,
    normalized_data: NormalizedProductData,
    pallet_calculation: Option<PalletCalculation>,
    validations: Vec<ValidationResult>,
}

#[derive(Debug, Serialize)]
struct ValidationStatuses {
    unit_conversion: Option<ValidationStatus>,
    net_vs_gross_weight: Option<ValidationStatus>,
    carton_weight_validation: Option<ValidationStatus>,
    width_depth_validation: Option<ValidationStatus>,
    volume_validation: Option<ValidationStatus>,
    ean13_validation: Option<ValidationStatus>,
    gtin14_validation: Option<ValidationStatus>,
    cases_per_layer_validation: Option<ValidationStatus>,
    pallet_height_validation: Option<ValidationStatus>,
    units_per_pallet_validation: Option<ValidationStatus>,
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
    calculated_volume_cm3: f64,
    declared_volume_cm3: Option<f64>,
    units_per_carton: u32,
    expected_minimum_carton_weight_g: f64,
}

#[derive(Debug, Serialize)]
struct PalletCalculation {
    layout_orientation: String,
    calculated_max_cases_per_layer: u32,
    used_cases_per_layer: u32,
    calculated_max_layers_by_height: u32,
    used_layers_per_pallet: u32,
    total_cases_per_pallet: u64,
    calculated_units_per_pallet: u64,
    declared_units_per_pallet: Option<u64>,
    pallet_base_height_cm: f64,
    load_height_cm: f64,
    total_pallet_height_cm: f64,
    max_total_height_cm: f64,
    remaining_height_cm: f64,
}

#[derive(Debug, Serialize)]
struct FlatValidationResponse {
    sku: String,
    overall_status: String,
    passed: usize,
    warnings: usize,
    failed: usize,
    unit_conversion_status: Option<String>,
    net_vs_gross_weight_status: Option<String>,
    carton_weight_status: Option<String>,
    width_depth_status: Option<String>,
    volume_status: Option<String>,
    ean13_status: Option<String>,
    gtin14_status: Option<String>,
    cases_per_layer_status: Option<String>,
    pallet_height_status: Option<String>,
    units_per_pallet_status: Option<String>,
    layout_orientation: Option<String>,
    calculated_max_cases_per_layer: Option<u32>,
    used_cases_per_layer: Option<u32>,
    calculated_max_layers_by_height: Option<u32>,
    used_layers_per_pallet: Option<u32>,
    total_cases_per_pallet: Option<u64>,
    calculated_units_per_pallet: Option<u64>,
    total_pallet_height_cm: Option<f64>,
    max_total_height_cm: Option<f64>,
    remaining_height_cm: Option<f64>,
    validation_report: String,
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
        .route("/api/v1/validate-product", post(validate_product_handler))
        .route("/api/v2/validate-products", post(validate_products_handler))
        .route("/api/v3/validate-product-json", post(validate_product_json_handler))
        .route("/api/v4/validate-product-flat", post(validate_product_flat_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "10000".to_string())
        .parse()
        .expect("PORT must be a valid number");

    let address = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Product Data Validator {} listening on {}", VERSION, address);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("Could not bind to address");
    axum::serve(listener, app).await.expect("Server failed");
}

async fn root() -> Json<Value> {
    Json(json!({
        "service": "Saether Product Data Validator",
        "version": VERSION,
        "status": "running",
        "endpoints": {
            "health": "/health",
            "validate_product": "/api/v1/validate-product",
            "validate_products_batch": "/api/v2/validate-products",
            "validate_product_json": "/api/v3/validate-product-json",
            "validate_product_flat": "/api/v4/validate-product-flat",
            "openapi": "/openapi.json"
        }
    }))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "product-data-validator",
        version: VERSION,
    })
}

async fn openapi() -> Json<Value> {
    Json(json!({
        "message": "Import the maintained Swagger file from the repository into Copilot Studio.",
        "version": VERSION
    }))
}

async fn validate_product_flat_handler(
    Json(request): Json<ProductJsonRequest>,
) -> Result<Json<FlatValidationResponse>, (StatusCode, Json<ApiError>)> {
    let product: ProductValidationRequest = serde_json::from_str(&request.product_json).map_err(
        |error| {
            bad_request(
                "product_json",
                &format!("product_json must contain valid product JSON: {}", error),
            )
        },
    )?;

    let response = validate_product(product)?;
    Ok(Json(flatten_response(response)))
}

async fn validate_product_json_handler(
    Json(request): Json<ProductJsonRequest>,
) -> Result<Json<ProductValidationResponse>, (StatusCode, Json<ApiError>)> {
    let product: ProductValidationRequest = serde_json::from_str(&request.product_json).map_err(
        |error| {
            bad_request(
                "product_json",
                &format!("product_json must contain valid product JSON: {}", error),
            )
        },
    )?;
    validate_product(product).map(Json)
}

async fn validate_product_handler(
    Json(product): Json<ProductValidationRequest>,
) -> Result<Json<ProductValidationResponse>, (StatusCode, Json<ApiError>)> {
    validate_product(product).map(Json)
}

async fn validate_products_handler(
    Json(request): Json<BatchValidationRequest>,
) -> Result<Json<BatchValidationResponse>, (StatusCode, Json<ApiError>)> {
    if request.products.is_empty() {
        return Err(bad_request("products", "At least one product is required."));
    }

    let mut products = Vec::with_capacity(request.products.len());
    for product in request.products {
        products.push(validate_product(product)?);
    }

    let passed = products
        .iter()
        .filter(|p| p.overall_status == ValidationStatus::Pass)
        .count();
    let warnings = products
        .iter()
        .filter(|p| p.overall_status == ValidationStatus::Warning)
        .count();
    let failed = products
        .iter()
        .filter(|p| p.overall_status == ValidationStatus::Fail)
        .count();

    Ok(Json(BatchValidationResponse {
        summary: BatchSummary {
            total_products: products.len(),
            passed,
            warnings,
            failed,
        },
        products,
    }))
}

fn validate_product(
    product: ProductValidationRequest,
) -> Result<ProductValidationResponse, (StatusCode, Json<ApiError>)> {
    validate_positive("net_weight", product.net_weight)?;
    validate_positive("gross_weight", product.gross_weight)?;
    validate_positive("width", product.width)?;
    validate_positive("depth", product.depth)?;
    validate_positive("height", product.height)?;
    validate_positive("carton_weight", product.carton_weight)?;
    if product.units_per_carton == 0 {
        return Err(bad_request("units_per_carton", "Must be greater than zero."));
    }

    let net_weight_g = convert_weight_to_g(product.net_weight, &product.weight_unit, "net_weight")?;
    let gross_weight_g = convert_weight_to_g(product.gross_weight, &product.weight_unit, "gross_weight")?;
    let carton_weight_g = convert_weight_to_g(
        product.carton_weight,
        &product.carton_weight_unit,
        "carton_weight",
    )?;
    let width_cm = convert_length_to_cm(product.width, &product.dimension_unit, "width")?;
    let depth_cm = convert_length_to_cm(product.depth, &product.dimension_unit, "depth")?;
    let height_cm = convert_length_to_cm(product.height, &product.dimension_unit, "height")?;
    let calculated_volume_cm3 = width_cm * depth_cm * height_cm;
    let declared_volume_cm3 = match (product.declared_volume, product.volume_unit.as_deref()) {
        (Some(value), Some(unit)) => Some(convert_volume_to_cm3(value, unit, "declared_volume")?),
        (Some(_), None) => {
            return Err(bad_request(
                "volume_unit",
                "volume_unit is required when declared_volume is supplied.",
            ))
        }
        (None, Some(_)) => {
            return Err(bad_request(
                "declared_volume",
                "declared_volume is required when volume_unit is supplied.",
            ))
        }
        (None, None) => None,
    };
    let expected_minimum_carton_weight_g = gross_weight_g * f64::from(product.units_per_carton);

    let mut validations = Vec::new();
    validations.push(result(
        "unit_conversion",
        ValidationStatus::Pass,
        "All supplied units were normalized successfully.",
        json!({
            "weight_unit_standard": "g",
            "dimension_unit_standard": "cm",
            "volume_unit_standard": "cm3"
        }),
    ));

    if gross_weight_g >= net_weight_g {
        validations.push(result(
            "net_vs_gross_weight",
            ValidationStatus::Pass,
            "Gross weight is not lower than net weight.",
            json!({"net_weight_g": round(net_weight_g), "gross_weight_g": round(gross_weight_g)}),
        ));
    } else {
        validations.push(result(
            "net_vs_gross_weight",
            ValidationStatus::Fail,
            "Gross weight is lower than net weight.",
            json!({"net_weight_g": round(net_weight_g), "gross_weight_g": round(gross_weight_g)}),
        ));
    }

    if carton_weight_g >= expected_minimum_carton_weight_g {
        validations.push(result(
            "carton_weight_validation",
            ValidationStatus::Pass,
            "Carton weight is not lower than the expected minimum.",
            json!({
                "actual_carton_weight_g": round(carton_weight_g),
                "expected_minimum_carton_weight_g": round(expected_minimum_carton_weight_g)
            }),
        ));
    } else {
        validations.push(result(
            "carton_weight_validation",
            ValidationStatus::Fail,
            "Carton weight is lower than the expected minimum.",
            json!({
                "actual_carton_weight_g": round(carton_weight_g),
                "expected_minimum_carton_weight_g": round(expected_minimum_carton_weight_g)
            }),
        ));
    }

    if width_cm >= depth_cm {
        validations.push(result(
            "width_depth_validation",
            ValidationStatus::Pass,
            "Width is greater than or equal to depth.",
            json!({"width_cm": round(width_cm), "depth_cm": round(depth_cm)}),
        ));
    } else {
        validations.push(result(
            "width_depth_validation",
            ValidationStatus::Fail,
            "Width must be greater than or equal to depth.",
            json!({"width_cm": round(width_cm), "depth_cm": round(depth_cm)}),
        ));
    }

    if let Some(declared) = declared_volume_cm3 {
        let tolerance = product.volume_tolerance_percent.unwrap_or(2.0);
        if tolerance < 0.0 {
            return Err(bad_request(
                "volume_tolerance_percent",
                "Must be zero or greater.",
            ));
        }
        let difference = (declared - calculated_volume_cm3).abs();
        let difference_percent = if calculated_volume_cm3 == 0.0 {
            0.0
        } else {
            difference / calculated_volume_cm3 * 100.0
        };
        let status = if difference_percent <= tolerance {
            ValidationStatus::Pass
        } else {
            ValidationStatus::Fail
        };
        let message = if status == ValidationStatus::Pass {
            "Declared volume is within the configured tolerance."
        } else {
            "Declared volume is outside the configured tolerance."
        };
        validations.push(result(
            "volume_validation",
            status,
            message,
            json!({
                "declared_volume_cm3": round(declared),
                "calculated_volume_cm3": round(calculated_volume_cm3),
                "difference_cm3": round(difference),
                "difference_percent": round(difference_percent),
                "tolerance_percent": round(tolerance)
            }),
        ));
    }

    if let Some(ref ean13) = product.ean13 {
        let valid = validate_gtin(ean13, 13);
        validations.push(result(
            "ean13_validation",
            if valid { ValidationStatus::Pass } else { ValidationStatus::Fail },
            if valid {
                "EAN-13 check digit is valid."
            } else {
                "EAN-13 must contain 13 digits and have a valid check digit."
            },
            json!({"value": ean13}),
        ));
    }

    if let Some(ref gtin14) = product.gtin14 {
        let valid = validate_gtin(gtin14, 14);
        validations.push(result(
            "gtin14_validation",
            if valid { ValidationStatus::Pass } else { ValidationStatus::Fail },
            if valid {
                "GTIN-14 check digit is valid."
            } else {
                "GTIN-14 must contain 14 digits and have a valid check digit."
            },
            json!({"value": gtin14}),
        ));
    }

    let pallet_calculation = match product.pallet.as_ref() {
        Some(pallet) => Some(validate_pallet(
            pallet,
            product.units_per_carton,
            &mut validations,
        )?),
        None => None,
    };

    let passed = validations
        .iter()
        .filter(|r| r.status == ValidationStatus::Pass)
        .count();
    let warnings = validations
        .iter()
        .filter(|r| r.status == ValidationStatus::Warning)
        .count();
    let failed = validations
        .iter()
        .filter(|r| r.status == ValidationStatus::Fail)
        .count();
    let overall_status = if failed > 0 {
        ValidationStatus::Fail
    } else if warnings > 0 {
        ValidationStatus::Warning
    } else {
        ValidationStatus::Pass
    };

    let validation_statuses = ValidationStatuses {
        unit_conversion: validation_status_for(&validations, "unit_conversion"),
        net_vs_gross_weight: validation_status_for(&validations, "net_vs_gross_weight"),
        carton_weight_validation: validation_status_for(&validations, "carton_weight_validation"),
        width_depth_validation: validation_status_for(&validations, "width_depth_validation"),
        volume_validation: validation_status_for(&validations, "volume_validation"),
        ean13_validation: validation_status_for(&validations, "ean13_validation"),
        gtin14_validation: validation_status_for(&validations, "gtin14_validation"),
        cases_per_layer_validation: validation_status_for(&validations, "cases_per_layer_validation"),
        pallet_height_validation: validation_status_for(&validations, "pallet_height_validation"),
        units_per_pallet_validation: validation_status_for(&validations, "units_per_pallet_validation"),
    };

    Ok(ProductValidationResponse {
        sku: product.sku,
        product_name: product.product_name,
        overall_status,
        summary: ValidationSummary {
            passed,
            warnings,
            failed,
        },
        validation_statuses,
        normalized_data: NormalizedProductData {
            net_weight_g: round(net_weight_g),
            gross_weight_g: round(gross_weight_g),
            carton_weight_g: round(carton_weight_g),
            width_cm: round(width_cm),
            depth_cm: round(depth_cm),
            height_cm: round(height_cm),
            calculated_volume_cm3: round(calculated_volume_cm3),
            declared_volume_cm3: declared_volume_cm3.map(round),
            units_per_carton: product.units_per_carton,
            expected_minimum_carton_weight_g: round(expected_minimum_carton_weight_g),
        },
        pallet_calculation,
        validations,
    })
}

fn validate_pallet(
    pallet: &PalletData,
    units_per_carton: u32,
    validations: &mut Vec<ValidationResult>,
) -> Result<PalletCalculation, (StatusCode, Json<ApiError>)> {
    for (field, value) in [
        ("pallet.carton_width", pallet.carton_width),
        ("pallet.carton_depth", pallet.carton_depth),
        ("pallet.carton_height", pallet.carton_height),
        ("pallet.pallet_width", pallet.pallet_width),
        ("pallet.pallet_depth", pallet.pallet_depth),
        ("pallet.pallet_base_height", pallet.pallet_base_height),
        ("pallet.max_total_height", pallet.max_total_height),
    ] {
        validate_positive(field, value)?;
    }

    if matches!(pallet.cases_per_layer, Some(0)) {
        return Err(bad_request("pallet.cases_per_layer", "Must be greater than zero."));
    }
    if matches!(pallet.layers_per_pallet, Some(0)) {
        return Err(bad_request("pallet.layers_per_pallet", "Must be greater than zero."));
    }

    let carton_width_cm = convert_length_to_cm(
        pallet.carton_width,
        &pallet.height_unit,
        "pallet.carton_width",
    )?;
    let carton_depth_cm = convert_length_to_cm(
        pallet.carton_depth,
        &pallet.height_unit,
        "pallet.carton_depth",
    )?;
    let carton_height_cm = convert_length_to_cm(
        pallet.carton_height,
        &pallet.height_unit,
        "pallet.carton_height",
    )?;
    let pallet_width_cm = convert_length_to_cm(
        pallet.pallet_width,
        &pallet.height_unit,
        "pallet.pallet_width",
    )?;
    let pallet_depth_cm = convert_length_to_cm(
        pallet.pallet_depth,
        &pallet.height_unit,
        "pallet.pallet_depth",
    )?;
    let pallet_base_height_cm = convert_length_to_cm(
        pallet.pallet_base_height,
        &pallet.height_unit,
        "pallet.pallet_base_height",
    )?;
    let max_total_height_cm = convert_length_to_cm(
        pallet.max_total_height,
        &pallet.height_unit,
        "pallet.max_total_height",
    )?;

    if max_total_height_cm <= pallet_base_height_cm {
        return Err(bad_request(
            "pallet.max_total_height",
            "Must be greater than pallet_base_height.",
        ));
    }

    let normal = ((pallet_width_cm / carton_width_cm).floor() as u32)
        .saturating_mul((pallet_depth_cm / carton_depth_cm).floor() as u32);
    let rotated = ((pallet_width_cm / carton_depth_cm).floor() as u32)
        .saturating_mul((pallet_depth_cm / carton_width_cm).floor() as u32);
    let (calculated_max_cases_per_layer, layout_orientation) = if rotated > normal {
        (rotated, "ROTATED")
    } else {
        (normal, "STANDARD")
    };

    if calculated_max_cases_per_layer == 0 {
        return Err(bad_request(
            "pallet",
            "The carton footprint does not fit on the pallet footprint.",
        ));
    }

    let available_load_height_cm = max_total_height_cm - pallet_base_height_cm;
    let calculated_max_layers_by_height =
        (available_load_height_cm / carton_height_cm).floor() as u32;
    if calculated_max_layers_by_height == 0 {
        return Err(bad_request(
            "pallet",
            "No carton layer fits within the configured maximum total height.",
        ));
    }

    let used_cases_per_layer = pallet
        .cases_per_layer
        .unwrap_or(calculated_max_cases_per_layer);
    let cases_status = if used_cases_per_layer <= calculated_max_cases_per_layer {
        ValidationStatus::Pass
    } else {
        ValidationStatus::Fail
    };
    let cases_message = match pallet.cases_per_layer {
        None => "Cases per layer were calculated automatically from carton and pallet dimensions.",
        Some(_) if cases_status == ValidationStatus::Pass => {
            "Declared cases per layer fit within the calculated pallet footprint capacity."
        }
        Some(_) => "Declared cases per layer exceed the calculated pallet footprint capacity.",
    };
    validations.push(result(
        "cases_per_layer_validation",
        cases_status,
        cases_message,
        json!({
            "declared_cases_per_layer": pallet.cases_per_layer,
            "calculated_max_cases_per_layer": calculated_max_cases_per_layer,
            "used_cases_per_layer": used_cases_per_layer,
            "layout_orientation": layout_orientation,
            "carton_width_cm": round(carton_width_cm),
            "carton_depth_cm": round(carton_depth_cm),
            "pallet_width_cm": round(pallet_width_cm),
            "pallet_depth_cm": round(pallet_depth_cm)
        }),
    ));

    let used_layers_per_pallet = pallet
        .layers_per_pallet
        .unwrap_or(calculated_max_layers_by_height);
    let load_height_cm = f64::from(used_layers_per_pallet) * carton_height_cm;
    let total_pallet_height_cm = pallet_base_height_cm + load_height_cm;
    let remaining_height_cm = max_total_height_cm - total_pallet_height_cm;
    let height_status = if total_pallet_height_cm <= max_total_height_cm {
        ValidationStatus::Pass
    } else {
        ValidationStatus::Fail
    };
    let height_message = match pallet.layers_per_pallet {
        None => "Layers per pallet were calculated automatically from the available pallet height.",
        Some(_) if height_status == ValidationStatus::Pass => {
            "Calculated total pallet height is within the configured maximum."
        }
        Some(_) => "Calculated total pallet height exceeds the configured maximum.",
    };
    validations.push(result(
        "pallet_height_validation",
        height_status,
        height_message,
        json!({
            "declared_layers_per_pallet": pallet.layers_per_pallet,
            "calculated_max_layers_by_height": calculated_max_layers_by_height,
            "used_layers_per_pallet": used_layers_per_pallet,
            "pallet_base_height_cm": round(pallet_base_height_cm),
            "load_height_cm": round(load_height_cm),
            "total_pallet_height_cm": round(total_pallet_height_cm),
            "max_total_height_cm": round(max_total_height_cm),
            "remaining_height_cm": round(remaining_height_cm)
        }),
    ));

    let total_cases_per_pallet =
        u64::from(used_cases_per_layer) * u64::from(used_layers_per_pallet);
    let calculated_units_per_pallet =
        total_cases_per_pallet * u64::from(units_per_carton);
    let units_status = match pallet.declared_units_per_pallet {
        Some(declared) if declared != calculated_units_per_pallet => ValidationStatus::Fail,
        _ => ValidationStatus::Pass,
    };
    let units_message = match pallet.declared_units_per_pallet {
        None => "Units per pallet were calculated automatically.",
        Some(_) if units_status == ValidationStatus::Pass => {
            "Declared units per pallet match the calculated quantity."
        }
        Some(_) => "Declared units per pallet do not match the calculated quantity.",
    };
    validations.push(result(
        "units_per_pallet_validation",
        units_status,
        units_message,
        json!({
            "units_per_carton": units_per_carton,
            "cases_per_layer": used_cases_per_layer,
            "layers_per_pallet": used_layers_per_pallet,
            "total_cases_per_pallet": total_cases_per_pallet,
            "calculated_units_per_pallet": calculated_units_per_pallet,
            "declared_units_per_pallet": pallet.declared_units_per_pallet
        }),
    ));

    Ok(PalletCalculation {
        layout_orientation: layout_orientation.to_string(),
        calculated_max_cases_per_layer,
        used_cases_per_layer,
        calculated_max_layers_by_height,
        used_layers_per_pallet,
        total_cases_per_pallet,
        calculated_units_per_pallet,
        declared_units_per_pallet: pallet.declared_units_per_pallet,
        pallet_base_height_cm: round(pallet_base_height_cm),
        load_height_cm: round(load_height_cm),
        total_pallet_height_cm: round(total_pallet_height_cm),
        max_total_height_cm: round(max_total_height_cm),
        remaining_height_cm: round(remaining_height_cm),
    })
}

fn flatten_response(response: ProductValidationResponse) -> FlatValidationResponse {
    let validation_report = response
        .validations
        .iter()
        .map(|validation| {
            let details = validation
                .details
                .as_ref()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "{}".to_string());
            format!(
                "[{}] {}: {} | details: {}",
                status_text(&validation.status),
                validation.rule,
                validation.message,
                details
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let pallet = response.pallet_calculation.as_ref();

    FlatValidationResponse {
        sku: response.sku,
        overall_status: status_text(&response.overall_status).to_string(),
        passed: response.summary.passed,
        warnings: response.summary.warnings,
        failed: response.summary.failed,
        unit_conversion_status: status_option_text(response.validation_statuses.unit_conversion),
        net_vs_gross_weight_status: status_option_text(response.validation_statuses.net_vs_gross_weight),
        carton_weight_status: status_option_text(response.validation_statuses.carton_weight_validation),
        width_depth_status: status_option_text(response.validation_statuses.width_depth_validation),
        volume_status: status_option_text(response.validation_statuses.volume_validation),
        ean13_status: status_option_text(response.validation_statuses.ean13_validation),
        gtin14_status: status_option_text(response.validation_statuses.gtin14_validation),
        cases_per_layer_status: status_option_text(response.validation_statuses.cases_per_layer_validation),
        pallet_height_status: status_option_text(response.validation_statuses.pallet_height_validation),
        units_per_pallet_status: status_option_text(response.validation_statuses.units_per_pallet_validation),
        layout_orientation: pallet.map(|value| value.layout_orientation.clone()),
        calculated_max_cases_per_layer: pallet.map(|value| value.calculated_max_cases_per_layer),
        used_cases_per_layer: pallet.map(|value| value.used_cases_per_layer),
        calculated_max_layers_by_height: pallet.map(|value| value.calculated_max_layers_by_height),
        used_layers_per_pallet: pallet.map(|value| value.used_layers_per_pallet),
        total_cases_per_pallet: pallet.map(|value| value.total_cases_per_pallet),
        calculated_units_per_pallet: pallet.map(|value| value.calculated_units_per_pallet),
        total_pallet_height_cm: pallet.map(|value| value.total_pallet_height_cm),
        max_total_height_cm: pallet.map(|value| value.max_total_height_cm),
        remaining_height_cm: pallet.map(|value| value.remaining_height_cm),
        validation_report,
    }
}

fn status_option_text(status: Option<ValidationStatus>) -> Option<String> {
    status.map(|value| status_text(&value).to_string())
}

fn status_text(status: &ValidationStatus) -> &'static str {
    match status {
        ValidationStatus::Pass => "PASS",
        ValidationStatus::Warning => "WARNING",
        ValidationStatus::Fail => "FAIL",
    }
}

fn validation_status_for(
    validations: &[ValidationResult],
    rule_name: &str,
) -> Option<ValidationStatus> {
    validations
        .iter()
        .find(|validation| validation.rule == rule_name)
        .map(|validation| validation.status.clone())
}

fn result(rule: &str, status: ValidationStatus, message: &str, details: Value) -> ValidationResult {
    ValidationResult {
        rule: rule.to_string(),
        status,
        message: message.to_string(),
        details: Some(details),
    }
}

fn validate_gtin(value: &str, expected_length: usize) -> bool {
    if !matches!(expected_length, 13 | 14)
        || value.len() != expected_length
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    let digits: Vec<u32> = value.bytes().map(|b| u32::from(b - b'0')).collect();
    let supplied_check_digit = digits[expected_length - 1];
    let weighted_sum: u32 = digits[..expected_length - 1]
        .iter()
        .rev()
        .enumerate()
        .map(|(index_from_right, digit)| {
            let weight = if index_from_right % 2 == 0 { 3 } else { 1 };
            digit * weight
        })
        .sum();
    let calculated_check_digit = (10 - (weighted_sum % 10)) % 10;
    calculated_check_digit == supplied_check_digit
}

fn convert_weight_to_g(
    value: f64,
    unit: &str,
    field: &str,
) -> Result<f64, (StatusCode, Json<ApiError>)> {
    match normalize_unit(unit).as_str() {
        "g" | "gr" | "gram" | "grams" => Ok(value),
        "kg" | "kilogram" | "kilograms" => Ok(value * 1000.0),
        "mg" | "milligram" | "milligrams" => Ok(value / 1000.0),
        "oz" | "ounce" | "ounces" => Ok(value * 28.349_523_125),
        "lb" | "lbs" | "pound" | "pounds" => Ok(value * 453.592_37),
        unsupported => Err(bad_request(
            field,
            &format!(
                "Unsupported weight unit '{}'. Supported units are g, kg, mg, oz, lb/lbs.",
                unsupported
            ),
        )),
    }
}

fn convert_length_to_cm(
    value: f64,
    unit: &str,
    field: &str,
) -> Result<f64, (StatusCode, Json<ApiError>)> {
    match normalize_unit(unit).as_str() {
        "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => Ok(value),
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => Ok(value / 10.0),
        "m" | "meter" | "meters" | "metre" | "metres" => Ok(value * 100.0),
        "in" | "inch" | "inches" | "\"" => Ok(value * 2.54),
        unsupported => Err(bad_request(
            field,
            &format!(
                "Unsupported dimension unit '{}'. Supported units are mm, cm, m, in/inch.",
                unsupported
            ),
        )),
    }
}

fn convert_volume_to_cm3(
    value: f64,
    unit: &str,
    field: &str,
) -> Result<f64, (StatusCode, Json<ApiError>)> {
    match normalize_unit(unit).as_str() {
        "cm3" | "cm³" | "cc" | "ml" | "milliliter" | "milliliters" | "millilitre" | "millilitres" => Ok(value),
        "l" | "ltr" | "liter" | "liters" | "litre" | "litres" => Ok(value * 1000.0),
        "m3" | "m³" => Ok(value * 1_000_000.0),
        unsupported => Err(bad_request(
            field,
            &format!(
                "Unsupported volume unit '{}'. Supported units are cm3, ml, l/ltr, and m3.",
                unsupported
            ),
        )),
    }
}

fn validate_positive(
    field: &str,
    value: f64,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if !value.is_finite() || value <= 0.0 {
        Err(bad_request(field, "Must be a finite number greater than zero."))
    } else {
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::validate_gtin;

    #[test]
    fn accepts_known_valid_ean13() {
        assert!(validate_gtin("4006381333931", 13));
    }

    #[test]
    fn rejects_invalid_ean13() {
        assert!(!validate_gtin("4006381333930", 13));
    }

    #[test]
    fn accepts_known_valid_gtin14() {
        assert!(validate_gtin("04006381333931", 14));
    }

    #[test]
    fn rejects_invalid_gtin14() {
        assert!(!validate_gtin("15701234567899", 14));
    }
}
