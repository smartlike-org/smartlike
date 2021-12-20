use crate::DonationReceipt;
use actix_web::web;
use reqwest;
use reqwest::header::USER_AGENT;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// for sandbox testing - static PAYPAL_IPN: &str = "https://ipnpb.sandbox.paypal.com/cgi-bin/webscr";
static PAYPAL_IPN: &str = "https://ipnpb.paypal.com/cgi-bin/webscr";

pub async fn parse(
    query_string: String,
    query: web::Query<HashMap<String, String>>,
) -> anyhow::Result<DonationReceipt> {
    let receipt = parse_ipn(&query)?;
    match verify(&query_string).await {
        Ok(_) => {
            return Ok(receipt);
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Verification error: {}", e));
        }
    }
}

fn parse_ipn(params: &web::Query<HashMap<String, String>>) -> anyhow::Result<DonationReceipt> {
    check_required_fields(
        params,
        &[
            "invoice".to_string(),
            "item_name".to_string(),
            "item_number".to_string(),
            "business".to_string(),
            "payer_status".to_string(),
            "payment_status".to_string(),
            "payment_type".to_string(),
            "mc_gross".to_string(),
            "mc_fee".to_string(),
            "mc_currency".to_string(),
            "custom".to_string(),
        ],
    )?;

    assert_parameter(params, "txn_type", "web_accept")?;
    assert_parameter(params, "payment_type", "instant")?;
    assert_parameter(params, "payment_status", "Completed")?;

    let subject: Vec<&str> = params["item_name"].split(' ').collect();
    if subject.len() != 3 || subject[0] != "Donate" || subject[1] != "to" {
        return Err(anyhow::anyhow!(
            "Wrong item name: {}. Must start with \"Donate to \"",
            params["item_name"]
        ));
    }

    let custom: Vec<&str> = params["custom"].split(' ').collect();
    if custom.len() != 3 {
        return Err(anyhow::anyhow!(
            "Failed to parse custom field: {}",
            params["custom"]
        ));
    }

    let f_amount = params["mc_gross"]
        .parse::<f64>()
        .map_err(|_err| anyhow::anyhow!("failed to parse mc_gross parameter"))?;
    let f_fee = params["mc_fee"]
        .parse::<f64>()
        .map_err(|_err| anyhow::anyhow!("failed to parse mc_fee parameter"))?;
    let amount = f_amount - f_fee;

    let now = SystemTime::now();
    let ts = now.duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;

    Ok(DonationReceipt {
        donor: custom[0].to_string(),
        recipient: custom[1].to_string(),
        channel_id: custom[1].to_string(),
        alias: subject[2].to_string(),
        id: params["invoice"].to_string(),
        address: params["business"].to_string(),
        processor: "PayPal".to_string(),
        amount: amount,
        currency: params["mc_currency"].to_string(),
        target_currency: custom[2].to_string(),
        ts: ts,
    })
}

fn check_required_fields(
    params: &HashMap<String, String>,
    fields: &[String],
) -> anyhow::Result<()> {
    for f in fields {
        if params.contains_key(f) == false {
            println!("Missing field: {}", f);
            return Err(anyhow::anyhow!("Missing field: {}", f));
        }
    }
    Ok(())
}

fn assert_parameter(
    params: &HashMap<String, String>,
    name: &str,
    expected: &str,
) -> anyhow::Result<()> {
    if params.contains_key(name) == false || params[name] != expected {
        Err(anyhow::anyhow!(
            "Wrong ipn parameter {}={}. Expected \"{}\"",
            name,
            params[name],
            expected
        ))
    } else {
        Ok(())
    }
}

async fn verify(message: &str) -> Result<(), String> {
    let body: String = format!("cmd=_notify-validate&{}", message);
    println!("Sending {}", body);
    let client = reqwest::Client::new();
    let resp = client
        .post(PAYPAL_IPN)
        .header(USER_AGENT, "PHP-IPN-VerificationScript")
        .body(body)
        .send()
        .await
        .map_err(|err| format!("Send error: {}", err.to_string()).to_string())?
        .text()
        .await
        .map_err(|err| format!("Send error: {}", err.to_string()).to_string())?;

    println!("{}", resp);
    if resp == "VERIFIED" {
        Ok(())
    } else {
        Err(format!("Failed to verify: {}", resp).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argument_parsing() {
        let receipt = parse_ipn(&web::Query::from_query("mc_gross=100.00&invoice=.XXXXXXXXXX&protection_eligibility=Eligible&payer_id=XXXXXXXXX&payment_date=07%3A25%3A25+May+13%2C+2021+PDT&payment_status=Completed&charset=KOI8_R&first_name=XXXXXX&mc_fee=14.40&notify_version=3.9&custom=6451b474b8ed84b5ad2d6f834f454d9800341e0f04c9ae8e40b9911dffa38cbb+d40bebba-aa61-8b6a-62b6-cd265df42796+EUR&payer_status=verified&business=xxxxxxx%40gmail.com&quantity=1&verify_sign=XXXXXXXX.XXXXXXX&payer_email=XXXXXXX%40example.com&txn_id=XXXXXXXXXX&payment_type=instant&payer_business_name=XXXXXXXX&last_name=XXXXXXXX&receiver_email=XXXXXXXX%40example.com&payment_fee=&shipping_discount=0.00&receiver_id=XXXXXXXXXXX&insurance_amount=0.00&txn_type=web_accept&item_name=Donate+to+d40bebba-aa61-8b6a-62b6-cd265df42796&discount=0.00&mc_currency=RUB&item_number=&residence_country=AT&shipping_method=Default&transaction_subject=6451b474b8ed84b5ad2d6f834f454d9800341e0f04c9ae8e40b9911dffa38cbb+d40bebba-aa61-8b6a-62b6-cd265df42796+EUR&payment_gross=&ipn_track_id=XXXXXXXXX").unwrap());
        assert_eq!(receipt.is_ok(), true);
        let receipt = receipt.unwrap();
        assert_eq!(
            receipt.donor,
            "6451b474b8ed84b5ad2d6f834f454d9800341e0f04c9ae8e40b9911dffa38cbb"
        );
        assert_eq!(receipt.recipient, "d40bebba-aa61-8b6a-62b6-cd265df42796");
        assert_eq!(receipt.channel_id, "d40bebba-aa61-8b6a-62b6-cd265df42796");
        assert_eq!(receipt.alias, "d40bebba-aa61-8b6a-62b6-cd265df42796");
        assert_eq!(receipt.id, ".XXXXXXXXXX");
        assert_eq!(receipt.address, "xxxxxxx@gmail.com");
        assert_eq!(receipt.processor, "PayPal");
        assert_eq!(receipt.amount, 85.6);
        assert_eq!(receipt.currency, "RUB");
        assert_eq!(receipt.target_currency, "EUR");
    }
}
