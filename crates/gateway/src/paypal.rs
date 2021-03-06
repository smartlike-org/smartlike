use crate::DonationReceipt;
use actix_web::web;
use anyhow::anyhow;
use reqwest;
use reqwest::header::USER_AGENT;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid;

// for sandbox testing - const PAYPAL_IPN: &str = "https://ipnpb.sandbox.paypal.com/cgi-bin/webscr";
const PAYPAL_IPN: &str = "https://ipnpb.paypal.com/cgi-bin/webscr";
const REQUIRED_PARAMETERS: [&str; 9] = [
    "receiver_email",
    "payer_status",
    "payment_status",
    "payment_type",
    "mc_gross",
    "mc_fee",
    "mc_currency",
    "txn_type",
    "txn_id",
];

macro_rules! assert_parameter {
    ($map:ident, $name:literal, $expect:literal) => {
        if $map.contains_key($name) == false {
            return Err(anyhow!("Missing parameter {}", $name));
        } else if $map[$name] != $expect {
            return Err(anyhow!(
                "Incorrect ipn parameter '{}={}'. Expected '{}'",
                $name,
                $map[$name],
                $expect
            ));
        }
    };
}

pub async fn parse(
    query_string: &str,
    query: web::Query<HashMap<String, String>>,
) -> anyhow::Result<DonationReceipt> {
    debug!("Received {}", query_string);
    match verify(query_string).await {
        Ok(verified) => {
            if verified {
                parse_ipn(&query)
            } else {
                Err(anyhow!("IPN not verified"))
            }
        }
        Err(e) => Err(anyhow!("Verification error: {}", e)),
    }
}

fn parse_ipn(params: &web::Query<HashMap<String, String>>) -> anyhow::Result<DonationReceipt> {
    let missing: Vec<&str> = REQUIRED_PARAMETERS
        .iter()
        .flat_map(|&k| {
            if !params.contains_key(k) {
                Some(k)
            } else {
                None
            }
        })
        .collect();
    if missing.len() > 0 {
        return Err(anyhow!("Missing field(s): {}", missing.join(", ")));
    }

    assert_parameter!(params, "payment_type", "instant");
    assert_parameter!(params, "payment_status", "Completed");

    if params["txn_type"] != "web_accept"
        && params["txn_type"] != "recurring_payment"
        && params["txn_type"] != "send_money"
    {
        return Err(anyhow!(
            "Wrong ipn parameter txn_type = \"{}\"",
            params["txn_type"],
        ));
    }

    // Donate+to+4855e1d3-ac4a-f6c4-8e03-f66001cef053+from+256bd4c260ee7d9554cf926a5120d0632b149f54a86ac65b660198b4c42c292d+USD
    let mut data = "".to_string();
    if params.contains_key("product_name") && params["product_name"].len() > 100 {
        data = params["product_name"].to_string();
    } else if params.contains_key("transaction_subject")
        && params["transaction_subject"].len() > 100
    {
        data = params["transaction_subject"].to_string();
    } else if params.contains_key("item_name") && params["item_name"].len() > 100 {
        data = params["item_name"].to_string();
    }
    let custom: Vec<&str> = data.split(' ').collect();
    if custom.len() != 6
        || custom[0] != "Donate"
        || custom[1] != "to"
        || custom[3] != "from"
        || uuid::Uuid::parse_str(custom[2]).is_ok() == false
        || custom[4].len() < 64
    {
        return Err(anyhow!("Not a Smartlike notification: {}.", data));
    }
    let f_amount = params["mc_gross"]
        .parse::<f64>()
        .map_err(|err| anyhow!("failed to parse mc_gross parameter: {}", err))?;
    let f_fee = params["mc_fee"]
        .parse::<f64>()
        .map_err(|err| anyhow!("failed to parse mc_fee parameter: {}", err))?;
    let amount = f_amount - f_fee;

    let now = SystemTime::now();
    let ts = now.duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;

    // Donate+to+4855e1d3-ac4a-f6c4-8e03-f66001cef053+from+256bd4c260ee7d9554cf926a5120d0632b149f54a86ac65b660198b4c42c292d+USD
    Ok(DonationReceipt {
        donor: custom[4].to_string(),
        recipient: custom[2].to_string(),
        channel_id: custom[2].to_string(), // reserved
        alias: "".to_string(),             // reserved
        id: params["txn_id"].to_string(),
        address: params["receiver_email"].to_string(),
        processor: "PayPal".to_string(),
        amount: amount,
        currency: params["mc_currency"].to_string(),
        target_currency: custom[5].to_string(),
        ts: ts,
    })
}

async fn verify(message: &str) -> anyhow::Result<bool> {
    let body: String = format!("cmd=_notify-validate&{}", message);
    debug!("Sending {}", body);
    let client = reqwest::Client::new();
    let resp = client
        .post(PAYPAL_IPN)
        .header(USER_AGENT, "PHP-IPN-VerificationScript")
        .body(body)
        .send()
        .await
        .map_err(|err| anyhow!("Send error: {}", err))?
        .text()
        .await
        .map_err(|err| anyhow!("Error getting HTTP response: {}", err))?;

    debug!("{}", resp);
    Ok(resp == "VERIFIED")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argument_parsing() {
        // One-time donation
        let receipt = parse_ipn(&web::Query::from_query("mc_gross=100.00&invoice_number=213341354543524&protection_eligibility=Eligible&payer_id=XXXXXXXXX&payment_date=07%3A25%3A25+May+13%2C+2021+PDT&payment_status=Completed&charset=KOI8_R&first_name=XXXXXX&mc_fee=14.40&notify_version=3.9&payer_status=verified&business=xxxxxxx%40gmail.com&quantity=1&verify_sign=XXXXXXXX.XXXXXXX&payer_email=XXXXXXX%40example.com&txn_id=XXXXXXXXXX&payment_type=instant&payer_business_name=XXXXXXXX&last_name=XXXXXXXX&receiver_email=XXXXXXXX%40example.com&payment_fee=&shipping_discount=0.00&receiver_id=XXXXXXXXXXX&insurance_amount=0.00&txn_type=web_accept&transaction_subject=Donate+to+4855e1d3-ac4a-f6c4-8e03-f66001cef053+from+256bd4c260ee7d9554cf926a5120d0632b149f54a86ac65b660198b4c42c292d+EUR&discount=0.00&mc_currency=RUB&item_number=&residence_country=AT&shipping_method=Default&payment_gross=&ipn_track_id=XXXXXXXXX").unwrap()).unwrap();
        assert_eq!(receipt.recipient, "4855e1d3-ac4a-f6c4-8e03-f66001cef053");
        assert_eq!(receipt.channel_id, "4855e1d3-ac4a-f6c4-8e03-f66001cef053");
        assert_eq!(
            receipt.donor,
            "256bd4c260ee7d9554cf926a5120d0632b149f54a86ac65b660198b4c42c292d"
        );
        assert_eq!(receipt.alias, "");
        assert_eq!(receipt.id, "XXXXXXXXXX");
        assert_eq!(receipt.address, "XXXXXXXX@example.com");
        assert_eq!(receipt.processor, "PayPal");
        assert_eq!(receipt.amount, 85.6);
        assert_eq!(receipt.currency, "RUB");
        assert_eq!(receipt.target_currency, "EUR");

        // Recurring payment
        let receipt = parse_ipn(&web::Query::from_query("mc_gross=2.00&period_type=+Regular&outstanding_balance=0.00&next_payment_date=03%3A00%3A00+May+13%2C+2022+PDT&protection_eligibility=Ineligible&payment_cycle=Monthly&tax=0.00&payer_id=QWRKD4DDU87H2&payment_date=03%3A21%3A05+Apr+13%2C+2022+PDT&payment_status=Completed&product_name=Donate+to+4855e1d3-ac4a-f6c4-8e03-f66001cef053+from+6451b474b8ed84b5ad2d6f834f454d9800341e0f04c9ae8e40b9911dffa38cbb+EUR&charset=UTF-8&recurring_payment_id=XXXXXXXXX&first_name=XXXXXXX&mc_fee=0.46&notify_version=3.9&amount_per_cycle=2.00&payer_status=verified&currency_code=EUR&business=donate%40smartlike.org&verify_sign=XXXXXXXXXXXXXXXXX&payer_email=XXXXXXXX%40example.com&initial_payment_amount=0.00&profile_status=Active&amount=2.00&txn_id=XXXXXX&payment_type=instant&payer_business_name=XXXXXXXs&last_name=XXXXXXX&receiver_email=donate%40smartlike.org&payment_fee=&receiver_id=XXXXXXX&txn_type=recurring_payment&mc_currency=EUR&residence_country=US&transaction_subject=Donate+to+4855e1d3-ac4a-f6c4-8e03-f66001cef053+from+256bd4c260ee7d9554cf926a5120d0632b149f54a86ac65b660198b4c42c292d+EUR&payment_gross=&shipping=0.00&product_type=1&time_created=07%3A45%3A05+Mar+13%2C+2022+PDT&ipn_track_id=XXXXXXXXX").unwrap()).unwrap();
        assert_eq!(receipt.recipient, "4855e1d3-ac4a-f6c4-8e03-f66001cef053");

        // Missing parameters
        let result = parse_ipn(&web::Query::from_query("period_type=+Regular&outstanding_balance=0.00&next_payment_date=03%3A00%3A00+May+13%2C+2022+PDT&protection_eligibility=Ineligible&payment_cycle=Monthly&tax=0.00&payer_id=QWRKD4DDU87H2&payment_date=03%3A21%3A05+Apr+13%2C+2022+PDT&payment_status=Completed&product_name=Donate+to+4855e1d3-ac4a-f6c4-8e03-f66001cef053+from+6451b474b8ed84b5ad2d6f834f454d9800341e0f04c9ae8e40b9911dffa38cbb+EUR&charset=UTF-8&recurring_payment_id=XXXXXXXXX&first_name=XXXXXXX&mc_fee=0.46&notify_version=3.9&amount_per_cycle=2.00&payer_status=verified&currency_code=EUR&business=donate%40smartlike.org&verify_sign=XXXXXXXXXXXXXXXXX&payer_email=XXXXXXXX%40example.com&initial_payment_amount=0.00&profile_status=Active&amount=2.00&txn_id=XXXXXX&payment_type=instant&payer_business_name=XXXXXXXs&last_name=XXXXXXX&receiver_email=donate%40smartlike.org&payment_fee=&receiver_id=XXXXXXX&txn_type=recurring_payment&residence_country=US&transaction_subject=Donate+to+4855e1d3-ac4a-f6c4-8e03-f66001cef053+from+256bd4c260ee7d9554cf926a5120d0632b149f54a86ac65b660198b4c42c292d+EUR&payment_gross=&shipping=0.00&product_type=1&time_created=07%3A45%3A05+Mar+13%2C+2022+PDT&ipn_track_id=XXXXXXXXX").unwrap());
        assert_eq!(
            result.err().unwrap().to_string(),
            "Missing field(s): mc_gross, mc_currency"
        );

        // Incorrect parameter
        let result = parse_ipn(&web::Query::from_query("mc_gross=2.00&period_type=+Regular&outstanding_balance=0.00&next_payment_date=03%3A00%3A00+May+13%2C+2022+PDT&protection_eligibility=Ineligible&payment_cycle=Monthly&tax=0.00&payer_id=QWRKD4DDU87H2&payment_date=03%3A21%3A05+Apr+13%2C+2022+PDT&payment_status=Incorrect&product_name=Donate+to+4855e1d3-ac4a-f6c4-8e03-f66001cef053+from+6451b474b8ed84b5ad2d6f834f454d9800341e0f04c9ae8e40b9911dffa38cbb+EUR&charset=UTF-8&recurring_payment_id=XXXXXXXXX&first_name=XXXXXXX&mc_fee=0.46&notify_version=3.9&amount_per_cycle=2.00&payer_status=verified&currency_code=EUR&business=donate%40smartlike.org&verify_sign=XXXXXXXXXXXXXXXXX&payer_email=XXXXXXXX%40example.com&initial_payment_amount=0.00&profile_status=Active&amount=2.00&txn_id=XXXXXX&payment_type=instant&payer_business_name=XXXXXXXs&last_name=XXXXXXX&receiver_email=donate%40smartlike.org&payment_fee=&receiver_id=XXXXXXX&txn_type=recurring_payment&mc_currency=EUR&residence_country=US&transaction_subject=Donate+to+4855e1d3-ac4a-f6c4-8e03-f66001cef053+from+256bd4c260ee7d9554cf926a5120d0632b149f54a86ac65b660198b4c42c292d+EUR&payment_gross=&shipping=0.00&product_type=1&time_created=07%3A45%3A05+Mar+13%2C+2022+PDT&ipn_track_id=XXXXXXXXX").unwrap());
        assert_eq!(
            result.err().unwrap().to_string(),
            "Incorrect ipn parameter 'payment_status=Incorrect'. Expected 'Completed'"
        );
    }
}
