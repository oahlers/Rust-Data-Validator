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

#[derive(Debug, Deserialize, Clone)]
struct PalletData {
    cases_per_layer: u32,
    layers_per_pallet: u32,
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
    cases_per_layer: u32,
    calculated_max_cases_per_layer: u32,
    layers_per_pallet: u32,
    total_cases_per_pallet: u64,
    calculated_units_per_pallet: u64,
    declared_units_per_pallet: Option<u64>,
    carton_width_cm: f64,
    carton_depth_cm: f64,
    carton_height_cm: f64,
    pallet_width_cm: f64,
    pallet_depth_cm: f64,
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
        .route("/api/v1/validate-product", post(validate_product_handler))
        .route("/api/v2/validate-products", post(validate_products_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "10000".to_string())
        .parse()
        .expect("PORT must be a valid number");

    let address = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Product Data Validator V2 listening on {}", address);

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("Could not bind to address");

    axum::serve(listener, app).await.expect("Server failed");
}

async fn root() -> Json<Value> {
    Json(json!({
        "service": "Saether Product Data Validator",
        "version": "2.0.0",
        "status": "running",
        "endpoints": {
            "health": "/health",
            "validate_product": "/api/v1/validate-product",
            "validate_products_batch": "/api/v2/validate-products",
            "openapi": "/openapi.json"
        }
    }))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "product-data-validator",
        version: "2.0.0",
    })
}

async fn validate_product_handler(
    Json(product): Json<ProductValidationRequest>,
) -> Result<Json<ProductValidationResponse>, (StatusCode, Json<ApiError>)> {
    validate_product(product).map(Json)
}

async fn validate_products_handler(
    Json(batch): Json<BatchValidationRequest>,
) -> Result<Json<BatchValidationResponse>, (StatusCode, Json<ApiError>)> {
    if batch.products.is_empty() {
        return Err(bad_request("products", "The products array must not be empty."));
    }

    let mut products = Vec::with_capacity(batch.products.len());
    for product in batch.products {
        products.push(validate_product(product)?);
    }

    let passed = products.iter().filter(|p| p.overall_status == ValidationStatus::Pass).count();
    let warnings = products.iter().filter(|p| p.overall_status == ValidationStatus::Warning).count();
    let failed = products.iter().filter(|p| p.overall_status == ValidationStatus::Fail).count();

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
    validate_positive_value("net_weight", product.net_weight)?;
    validate_positive_value("gross_weight", product.gross_weight)?;
    validate_positive_value("width", product.width)?;
    validate_positive_value("depth", product.depth)?;
    validate_positive_value("height", product.height)?;
    validate_positive_value("carton_weight", product.carton_weight)?;

    if product.units_per_carton == 0 {
        return Err(bad_request("units_per_carton", "Units per carton must be greater than zero."));
    }

    let net_weight_g = convert_weight_to_grams(product.net_weight, &product.weight_unit, "weight_unit")?;
    let gross_weight_g = convert_weight_to_grams(product.gross_weight, &product.weight_unit, "weight_unit")?;
    let carton_weight_g = convert_weight_to_grams(product.carton_weight, &product.carton_weight_unit, "carton_weight_unit")?;

    let width_cm = convert_dimension_to_centimetres(product.width, &product.dimension_unit, "dimension_unit")?;
    let depth_cm = convert_dimension_to_centimetres(product.depth, &product.dimension_unit, "dimension_unit")?;
    let height_cm = convert_dimension_to_centimetres(product.height, &product.dimension_unit, "dimension_unit")?;

    let calculated_volume_cm3 = width_cm * depth_cm * height_cm;
    let declared_volume_cm3 = match (product.declared_volume, product.volume_unit.as_deref()) {
        (Some(value), Some(unit)) => {
            validate_positive_value("declared_volume", value)?;
            Some(convert_volume_to_cm3(value, unit, "volume_unit")?)
        }
        (None, None) => None,
        _ => return Err(bad_request(
            "declared_volume",
            "declared_volume and volume_unit must either both be supplied or both be omitted.",
        )),
    };

    let tolerance_percent = product.volume_tolerance_percent.unwrap_or(2.0);
    if !tolerance_percent.is_finite() || tolerance_percent < 0.0 {
        return Err(bad_request(
            "volume_tolerance_percent",
            "Volume tolerance must be zero or greater.",
        ));
    }

    let expected_minimum_carton_weight_g = gross_weight_g * product.units_per_carton as f64;
    let mut validations = Vec::new();

    validations.push(ValidationResult {
        rule: "unit_conversion".to_string(),
        status: ValidationStatus::Pass,
        message: "All supplied units were normalized successfully.".to_string(),
        details: Some(json!({
            "weight_unit_standard": "g",
            "dimension_unit_standard": "cm",
            "volume_unit_standard": "cm3"
        })),
    });

    if net_weight_g > gross_weight_g {
        validations.push(result(
            "net_vs_gross_weight",
            ValidationStatus::Fail,
            "Net weight exceeds gross weight.",
            json!({"net_weight_g": round(net_weight_g), "gross_weight_g": round(gross_weight_g)}),
        ));
    } else {
        validations.push(result(
            "net_vs_gross_weight",
            ValidationStatus::Pass,
            "Gross weight is not lower than net weight.",
            json!({"net_weight_g": round(net_weight_g), "gross_weight_g": round(gross_weight_g)}),
        ));
    }

    if carton_weight_g < expected_minimum_carton_weight_g {
        validations.push(result(
            "carton_weight_validation",
            ValidationStatus::Fail,
            "Carton weight is lower than the combined gross weight of its units.",
            json!({
                "expected_minimum_carton_weight_g": round(expected_minimum_carton_weight_g),
                "actual_carton_weight_g": round(carton_weight_g)
            }),
        ));
    } else {
        validations.push(result(
            "carton_weight_validation",
            ValidationStatus::Pass,
            "Carton weight is not lower than the expected minimum.",
            json!({
                "expected_minimum_carton_weight_g": round(expected_minimum_carton_weight_g),
                "actual_carton_weight_g": round(carton_weight_g)
            }),
        ));
    }

    if width_cm < depth_cm {
        validations.push(result(
            "width_depth_validation",
            ValidationStatus::Warning,
            "Width is smaller than depth. The values may be reversed.",
            json!({
                "current_width_cm": round(width_cm),
                "current_depth_cm": round(depth_cm),
                "suggested_width_cm": round(depth_cm),
                "suggested_depth_cm": round(width_cm)
            }),
        ));
    } else {
        validations.push(result(
            "width_depth_validation",
            ValidationStatus::Pass,
            "Width is greater than or equal to depth.",
            json!({"width_cm": round(width_cm), "depth_cm": round(depth_cm)}),
        ));
    }

    if let Some(declared) = declared_volume_cm3 {
        let difference = (declared - calculated_volume_cm3).abs();
        let difference_percent = if calculated_volume_cm3 == 0.0 {
            0.0
        } else {
            difference / calculated_volume_cm3 * 100.0
        };
        let status = if difference_percent <= tolerance_percent {
            ValidationStatus::Pass
        } else {
            ValidationStatus::Fail
        };
        let message = if status == ValidationStatus::Pass {
            "Declared volume is within the configured tolerance."
        } else {
            "Declared volume differs from width × depth × height beyond the configured tolerance."
        };
        validations.push(result(
            "volume_validation",
            status,
            message,
            json!({
                "calculated_volume_cm3": round(calculated_volume_cm3),
                "declared_volume_cm3": round(declared),
                "difference_cm3": round(difference),
                "difference_percent": round(difference_percent),
                "tolerance_percent": round(tolerance_percent)
            }),
        ));
    }

    if let Some(ref ean13) = product.ean13 {
        let valid = validate_gtin(ean13, 13);
        validations.push(result(
            "ean13_validation",
            if valid { ValidationStatus::Pass } else { ValidationStatus::Fail },
            if valid { "EAN-13 check digit is valid." } else { "EAN-13 must contain 13 digits and have a valid check digit." },
            json!({"value": ean13}),
        ));
    }

    if let Some(ref gtin14) = product.gtin14 {
        let valid = validate_gtin(gtin14, 14);
        validations.push(result(
            "gtin14_validation",
            if valid { ValidationStatus::Pass } else { ValidationStatus::Fail },
            if valid { "GTIN-14 check digit is valid." } else { "GTIN-14 must contain 14 digits and have a valid check digit." },
            json!({"value": gtin14}),
        ));
    }

    let pallet_calculation = if let Some(pallet) = product.pallet {
        if pallet.cases_per_layer == 0 {
            return Err(bad_request("pallet.cases_per_layer", "Cases per layer must be greater than zero."));
        }
        if pallet.layers_per_pallet == 0 {
            return Err(bad_request("pallet.layers_per_pallet", "Layers per pallet must be greater than zero."));
        }
        for (field, value) in [
            ("pallet.carton_width", pallet.carton_width),
            ("pallet.carton_depth", pallet.carton_depth),
            ("pallet.carton_height", pallet.carton_height),
            ("pallet.pallet_width", pallet.pallet_width),
            ("pallet.pallet_depth", pallet.pallet_depth),
            ("pallet.pallet_base_height", pallet.pallet_base_height),
            ("pallet.max_total_height", pallet.max_total_height),
        ] {
            validate_positive_value(field, value)?;
        }

        let carton_width_cm = convert_dimension_to_centimetres(pallet.carton_width, &pallet.height_unit, "pallet.height_unit")?;
        let carton_depth_cm = convert_dimension_to_centimetres(pallet.carton_depth, &pallet.height_unit, "pallet.height_unit")?;
        let carton_height_cm = convert_dimension_to_centimetres(pallet.carton_height, &pallet.height_unit, "pallet.height_unit")?;
        let pallet_width_cm = convert_dimension_to_centimetres(pallet.pallet_width, &pallet.height_unit, "pallet.height_unit")?;
        let pallet_depth_cm = convert_dimension_to_centimetres(pallet.pallet_depth, &pallet.height_unit, "pallet.height_unit")?;
        let pallet_base_height_cm = convert_dimension_to_centimetres(pallet.pallet_base_height, &pallet.height_unit, "pallet.height_unit")?;
        let max_total_height_cm = convert_dimension_to_centimetres(pallet.max_total_height, &pallet.height_unit, "pallet.height_unit")?;

        let orientation_a = ((pallet_width_cm / carton_width_cm).floor() as u32)
            .saturating_mul((pallet_depth_cm / carton_depth_cm).floor() as u32);
        let orientation_b = ((pallet_width_cm / carton_depth_cm).floor() as u32)
            .saturating_mul((pallet_depth_cm / carton_width_cm).floor() as u32);
        let calculated_max_cases_per_layer = orientation_a.max(orientation_b);

        let cases_status = if pallet.cases_per_layer <= calculated_max_cases_per_layer {
            ValidationStatus::Pass
        } else {
            ValidationStatus::Fail
        };
        let cases_message = if cases_status == ValidationStatus::Pass {
            "Cases per layer fit within the simple grid calculation."
        } else {
            "Declared cases per layer exceed the simple grid calculation for the supplied pallet and carton dimensions."
        };
        validations.push(result(
            "cases_per_layer_validation",
            cases_status,
            cases_message,
            json!({
                "declared_cases_per_layer": pallet.cases_per_layer,
                "calculated_max_cases_per_layer": calculated_max_cases_per_layer,
                "orientation_a": orientation_a,
                "orientation_b": orientation_b,
                "calculation_model": "simple rectangular grid; mixed-orientation packing is not calculated"
            }),
        ));

        let total_cases_per_pallet = pallet.cases_per_layer as u64 * pallet.layers_per_pallet as u64;
        let calculated_units_per_pallet = total_cases_per_pallet * product.units_per_carton as u64;
        let goods_height_cm = carton_height_cm * pallet.layers_per_pallet as f64;
        let total_pallet_height_cm = pallet_base_height_cm + goods_height_cm;
        let remaining_height_cm = max_total_height_cm - total_pallet_height_cm;

        let height_status = if total_pallet_height_cm <= max_total_height_cm {
            ValidationStatus::Pass
        } else {
            ValidationStatus::Fail
        };
        validations.push(result(
            "pallet_height_validation",
            height_status.clone(),
            if height_status == ValidationStatus::Pass {
                "Calculated pallet height is within the configured maximum total height."
            } else {
                "Calculated pallet height exceeds the configured maximum total height."
            },
            json!({
                "total_pallet_height_cm": round(total_pallet_height_cm),
                "max_total_height_cm": round(max_total_height_cm),
                "remaining_height_cm": round(remaining_height_cm)
            }),
        ));

        if let Some(declared_units) = pallet.declared_units_per_pallet {
            let units_status = if declared_units == calculated_units_per_pallet {
                ValidationStatus::Pass
            } else {
                ValidationStatus::Fail
            };
            validations.push(result(
                "units_per_pallet_validation",
                units_status.clone(),
                if units_status == ValidationStatus::Pass {
                    "Declared units per pallet match the packaging hierarchy calculation."
                } else {
                    "Declared units per pallet do not match units per carton × cases per layer × layers per pallet."
                },
                json!({
                    "declared_units_per_pallet": declared_units,
                    "calculated_units_per_pallet": calculated_units_per_pallet,
                    "units_per_carton": product.units_per_carton,
                    "cases_per_layer": pallet.cases_per_layer,
                    "layers_per_pallet": pallet.layers_per_pallet
                }),
            ));
        }

        Some(PalletCalculation {
            cases_per_layer: pallet.cases_per_layer,
            calculated_max_cases_per_layer,
            layers_per_pallet: pallet.layers_per_pallet,
            total_cases_per_pallet,
            calculated_units_per_pallet,
            declared_units_per_pallet: pallet.declared_units_per_pallet,
            carton_width_cm: round(carton_width_cm),
            carton_depth_cm: round(carton_depth_cm),
            carton_height_cm: round(carton_height_cm),
            pallet_width_cm: round(pallet_width_cm),
            pallet_depth_cm: round(pallet_depth_cm),
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
        summary: ValidationSummary { passed, warnings, failed },
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
    if !matches!(expected_length, 13 | 14) {
        return false;
    }

    if value.len() != expected_length || !value.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }

    let digits: Vec<u32> = value
        .bytes()
        .map(|b| u32::from(b - b'0'))
        .collect();

    let supplied_check_digit = digits[expected_length - 1];
    let data_digits = &digits[..expected_length - 1];

    let weighted_sum: u32 = data_digits
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

#[cfg(test)]
mod gtin_tests {
    use super::validate_gtin;

    #[test]
    fn accepts_known_valid_ean13_values() {
        assert!(validate_gtin("5901234123457", 13));
        assert!(validate_gtin("4006381333931", 13));
        assert!(validate_gtin("6291041500213", 13));
    }

    #[test]
    fn rejects_invalid_ean13_values() {
        assert!(!validate_gtin("5701234567893", 13));
        assert!(!validate_gtin("5701234567890", 13));
        assert!(!validate_gtin("5901234123450", 13));
    }

    #[test]
    fn accepts_correct_check_digit_for_570123456789() {
        assert!(validate_gtin("5701234567899", 13));
    }

    #[test]
    fn preserves_and_validates_leading_zeroes() {
        assert!(validate_gtin("0123456789012", 13));
    }

    #[test]
    fn rejects_wrong_length_and_non_digits() {
        assert!(!validate_gtin("123", 13));
        assert!(!validate_gtin("590123412345X", 13));
    }
}

fn validate_positive_value(field: &str, value: f64) -> Result<(), (StatusCode, Json<ApiError>)> {
    if !value.is_finite() || value <= 0.0 {
        return Err(bad_request(field, "The value must be a finite number greater than zero."));
    }
    Ok(())
}

fn convert_weight_to_grams(value: f64, unit: &str, field: &str) -> Result<f64, (StatusCode, Json<ApiError>)> {
    match normalize_unit(unit).as_str() {
        "g" | "gram" | "grams" => Ok(value),
        "kg" | "kilogram" | "kilograms" => Ok(value * 1000.0),
        "oz" | "ounce" | "ounces" => Ok(value * 28.349_523_125),
        unsupported => Err(bad_request(field, &format!("Unsupported weight unit '{}'. Supported units are g, kg, and oz.", unsupported))),
    }
}

fn convert_dimension_to_centimetres(value: f64, unit: &str, field: &str) -> Result<f64, (StatusCode, Json<ApiError>)> {
    match normalize_unit(unit).as_str() {
        "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => Ok(value),
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => Ok(value / 10.0),
        "m" | "meter" | "meters" | "metre" | "metres" => Ok(value * 100.0),
        "in" | "inch" | "inches" => Ok(value * 2.54),
        unsupported => Err(bad_request(field, &format!("Unsupported dimension unit '{}'. Supported units are mm, cm, m, in, and inch.", unsupported))),
    }
}

fn convert_volume_to_cm3(value: f64, unit: &str, field: &str) -> Result<f64, (StatusCode, Json<ApiError>)> {
    match normalize_unit(unit).as_str() {
        "cm3" | "cm³" => Ok(value),
        "ml" | "milliliter" | "milliliters" | "millilitre" | "millilitres" => Ok(value),
        "l" | "ltr" | "liter" | "liters" | "litre" | "litres" => Ok(value * 1000.0),
        "m3" | "m³" => Ok(value * 1_000_000.0),
        unsupported => Err(bad_request(field, &format!("Unsupported volume unit '{}'. Supported units are cm3, ml, l/ltr, and m3.", unsupported))),
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
            "description": "V2 validates product dimensions, weights, volume, EAN-13/GTIN-14, pallet height, cases per layer, units per pallet, and batch requests.",
            "version": "2.0.0"
        },
        "paths": {
            "/health": {"get": {"operationId": "HealthCheck", "responses": {"200": {"description": "API is healthy"}}}},
            "/api/v1/validate-product": {"post": {"operationId": "ValidateProduct", "summary": "Validate one product", "responses": {"200": {"description": "Validation completed"}, "400": {"description": "Invalid input"}}}},
            "/api/v2/validate-products": {"post": {"operationId": "ValidateProducts", "summary": "Validate multiple products", "responses": {"200": {"description": "Batch validation completed"}, "400": {"description": "Invalid input"}}}}
        }
    }))
}
