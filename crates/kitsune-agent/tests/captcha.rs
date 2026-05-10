use kitsune_agent::captcha::CaptchaKind;

#[test]
fn captcha_kind_from_dom_snippet_recaptcha_v2() {
    let dom = r#"<div class="g-recaptcha" data-sitekey="abc123"></div>"#;
    assert_eq!(CaptchaKind::detect_from_html(dom), Some(CaptchaKind::RecaptchaV2));
}

#[test]
fn captcha_kind_from_dom_snippet_hcaptcha() {
    let dom = r#"<div class="h-captcha" data-sitekey="xyz"></div>"#;
    assert_eq!(CaptchaKind::detect_from_html(dom), Some(CaptchaKind::HCaptcha));
}

#[test]
fn captcha_kind_from_dom_snippet_turnstile() {
    let dom = r#"<div class="cf-turnstile" data-sitekey="abc"></div>"#;
    assert_eq!(CaptchaKind::detect_from_html(dom), Some(CaptchaKind::CloudflareTurnstile));
}

#[test]
fn captcha_kind_none_on_clean_page() {
    let dom = r#"<form><input type="text" name="email"></form>"#;
    assert_eq!(CaptchaKind::detect_from_html(dom), None);
}
