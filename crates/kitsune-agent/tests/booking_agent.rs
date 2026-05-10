use kitsune_agent::agents::booking::{BookingCriteria, BookingPriority, FlightOffer};

#[test]
fn cheapest_ranking() {
    let offers = vec![
        FlightOffer { price_minor: 50000, currency: "EUR".into(), duration_mins: 120, stops: 0, airline: "LH".into(), booking_url: "https://a.com".into() },
        FlightOffer { price_minor: 35000, currency: "EUR".into(), duration_mins: 180, stops: 1, airline: "FR".into(), booking_url: "https://b.com".into() },
    ];
    let criteria = BookingCriteria { primary: BookingPriority::Cheapest, max_stops: None, max_price_minor: None };
    assert_eq!(criteria.rank(&offers).unwrap().price_minor, 35000);
}

#[test]
fn fastest_ranking() {
    let offers = vec![
        FlightOffer { price_minor: 50000, currency: "EUR".into(), duration_mins: 120, stops: 0, airline: "LH".into(), booking_url: "https://a.com".into() },
        FlightOffer { price_minor: 35000, currency: "EUR".into(), duration_mins: 180, stops: 1, airline: "FR".into(), booking_url: "https://b.com".into() },
    ];
    let criteria = BookingCriteria { primary: BookingPriority::Fastest, max_stops: None, max_price_minor: None };
    assert_eq!(criteria.rank(&offers).unwrap().duration_mins, 120);
}
