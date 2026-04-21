/// HTML page assets for the KitsuneEngine investor demo.
///
/// All pages are embedded at compile time so the binary is fully self-contained.

// ---------------------------------------------------------------------------
// Welcome page
// ---------------------------------------------------------------------------

pub const WELCOME_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>KitsuneEngine — Built in Rust. Grounded in trust.</title>
  <style>
    :root {
      --bg: #0D0D0F;
      --surface: #16161A;
      --elevated: #1E1E24;
      --border: #2A2A35;
      --accent: #6B5CE7;
      --accent-hover: #8B7CF8;
      --text: #F0EFF5;
      --muted: #9897A3;
      --success: #2ECC71;
      --warning: #F39C12;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      background: var(--bg);
      color: var(--text);
      font-family: 'Inter', system-ui, sans-serif;
      height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 40px 20px;
    }
    .hero { text-align: center; max-width: 680px; }
    .logo {
      font-size: 48px;
      font-weight: 800;
      background: linear-gradient(135deg, var(--accent), var(--accent-hover));
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
      margin-bottom: 16px;
    }
    .tagline {
      font-size: 20px;
      color: var(--muted);
      margin-bottom: 48px;
      line-height: 1.6;
    }
    .stats {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 24px;
      margin-bottom: 48px;
    }
    .stat-card {
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 12px;
      padding: 20px 28px;
      text-align: center;
      min-width: 160px;
    }
    .stat-value {
      font-size: 32px;
      font-weight: 700;
      color: var(--success);
      display: block;
      margin-bottom: 4px;
    }
    .stat-label { font-size: 13px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; }
    .badges {
      display: flex;
      flex-direction: column;
      gap: 12px;
      align-items: flex-start;
      margin: 0 auto;
      max-width: 380px;
      text-align: left;
    }
    .badge {
      display: grid;
      grid-template-columns: 20px 1fr;
      align-items: center;
      gap: 12px;
      font-size: 15px;
    }
    .badge-dot { width: 10px; height: 10px; border-radius: 50%; background: var(--success); flex-shrink: 0; }
    .nav-links {
      margin-top: 48px;
      display: grid;
      grid-template-columns: auto auto;
      gap: 16px;
      justify-content: center;
    }
    .btn {
      display: inline-block;
      padding: 12px 28px;
      border-radius: 8px;
      text-decoration: none;
      font-size: 15px;
      font-weight: 600;
      transition: opacity 0.2s;
    }
    .btn:hover { opacity: 0.85; }
    .btn-primary { background: var(--accent); color: #fff; }
    .btn-secondary { background: var(--elevated); color: var(--text); border: 1px solid var(--border); }
    #tracker-count { color: var(--success); font-weight: 700; }
  </style>
</head>
<body>
  <div class="hero">
    <div class="logo">🦊 KitsuneEngine</div>
    <p class="tagline">Built in Rust. Grounded in trust.<br>Your browser that never leaks.</p>

    <div class="stats">
      <div class="stat-card">
        <span class="stat-value" id="tracker-count">3</span>
        <span class="stat-label">Trackers Blocked</span>
      </div>
      <div class="stat-card">
        <span class="stat-value">100%</span>
        <span class="stat-label">Rust Memory Safety</span>
      </div>
      <div class="stat-card">
        <span class="stat-value">0</span>
        <span class="stat-label">Data Leaks</span>
      </div>
    </div>

    <div class="badges">
      <div class="badge"><div class="badge-dot"></div> Your data stays on your device — always</div>
      <div class="badge"><div class="badge-dot"></div> Anti-fingerprinting active on every page</div>
      <div class="badge"><div class="badge-dot"></div> Tracker blocking prevents third-party surveillance</div>
      <div class="badge"><div class="badge-dot"></div> AI agents run in a sandboxed process. No data escapes.</div>
    </div>

    <div class="nav-links">
      <a href="/shop" class="btn btn-primary">View Demo Shop →</a>
      <a href="/privacy" class="btn btn-secondary">Privacy Report</a>
    </div>
  </div>

  <script>
    // Attempt to load trackers — KitsuneEngine will block these and increment the counter
    const trackers = ['/api/google-analytics', '/api/doubleclick-tracker', '/api/track'];
    let blocked = 0;
    trackers.forEach(url => {
      fetch(url).then(() => {}).catch(() => {
        blocked++;
        document.getElementById('tracker-count').textContent = blocked;
      });
    });
  </script>
</body>
</html>"#;

// ---------------------------------------------------------------------------
// Shop page
// ---------------------------------------------------------------------------

pub const SHOP_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>KitsuneShop — Demo Store</title>
  <style>
    :root {
      --bg: #0D0D0F; --surface: #16161A; --elevated: #1E1E24;
      --border: #2A2A35; --accent: #6B5CE7; --accent-hover: #8B7CF8;
      --text: #F0EFF5; --muted: #9897A3; --success: #2ECC71; --warning: #F39C12;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { background: var(--bg); color: var(--text); font-family: 'Inter', system-ui, sans-serif; }
    header {
      background: var(--surface); border-bottom: 1px solid var(--border);
      padding: 16px 40px; display: grid; grid-template-columns: 1fr auto;
    }
    .brand { font-size: 20px; font-weight: 700; }
    .brand span { color: var(--accent); }
    .privacy-badge {
      display: block; align-items: center; gap: 8px;
      font-size: 13px; color: var(--success);
    }
    main { max-width: 1100px; margin: 0 auto; padding: 40px 20px; }
    h1 { font-size: 28px; font-weight: 700; margin-bottom: 8px; }
    .subtitle { color: var(--muted); margin-bottom: 32px; }
    .products {
      display: grid; grid-template-columns: repeat(4, 1fr); gap: 20px; margin-bottom: 60px;
    }
    .product-card {
      background: var(--surface); border: 1px solid var(--border);
      border-radius: 12px; overflow: hidden;
      transition: transform 0.2s, border-color 0.2s;
    }
    .product-card:hover { transform: translateY(-4px); border-color: var(--accent); }
    .product-thumb {
      height: 160px; display: block; align-items: center;
      justify-content: center; font-size: 48px;
    }
    .product-info { padding: 16px; }
    .product-name { font-size: 15px; font-weight: 600; margin-bottom: 4px; }
    .product-desc { font-size: 13px; color: var(--muted); margin-bottom: 12px; }
    .product-price { font-size: 18px; font-weight: 700; color: var(--accent); }
    .checkout-section {
      background: var(--surface); border: 1px solid var(--border);
      border-radius: 16px; padding: 40px; max-width: 520px;
    }
    .checkout-section h2 { font-size: 22px; font-weight: 700; margin-bottom: 24px; }
    .form-group { margin-bottom: 16px; }
    label { display: block; font-size: 13px; color: var(--muted); margin-bottom: 6px; text-transform: uppercase; letter-spacing: 0.05em; }
    input[type="text"], input[type="email"] {
      width: 100%; background: var(--elevated); border: 1px solid var(--border);
      border-radius: 8px; padding: 12px 16px; color: var(--text);
      font-size: 15px; outline: none; transition: border-color 0.2s;
    }
    input:focus { border-color: var(--accent); }
    .btn-submit {
      width: 100%; background: var(--accent); color: #fff; border: none;
      border-radius: 8px; padding: 14px; font-size: 16px; font-weight: 600;
      cursor: pointer; margin-top: 8px; transition: background 0.2s;
    }
    .btn-submit:hover { background: var(--accent-hover); }
    #checkout-result {
      margin-top: 16px; padding: 12px 16px; border-radius: 8px;
      background: var(--elevated); display: none;
    }
    .back-link { display: inline-block; margin-bottom: 24px; color: var(--muted); text-decoration: none; font-size: 14px; }
    .back-link:hover { color: var(--text); }
  </style>
</head>
<body>
  <header>
    <div class="brand">🦊 Kitsune<span>Shop</span></div>
    <div class="privacy-badge">🔒 Agent-assisted checkout — all trackers blocked</div>
  </header>
  <main>
    <a href="/" class="back-link">← Back to home</a>
    <h1>Featured Products</h1>
    <p class="subtitle">8 products · trackers blocked · your data stays private</p>

    <div class="products">
      <div class="product-card" data-product="1">
        <div class="product-thumb" style="background:#1a1a2e">🎧</div>
        <div class="product-info">
          <div class="product-name">Noise-Cancelling Headphones</div>
          <div class="product-desc">Studio-grade audio isolation</div>
          <div class="product-price">$299</div>
        </div>
      </div>
      <div class="product-card" data-product="2">
        <div class="product-thumb" style="background:#1a2e1a">💻</div>
        <div class="product-info">
          <div class="product-name">Mechanical Keyboard</div>
          <div class="product-desc">Cherry MX switches, RGB</div>
          <div class="product-price">$149</div>
        </div>
      </div>
      <div class="product-card" data-product="3">
        <div class="product-thumb" style="background:#2e1a1a">🖥️</div>
        <div class="product-info">
          <div class="product-name">4K Monitor</div>
          <div class="product-desc">27-inch IPS, 144Hz refresh</div>
          <div class="product-price">$699</div>
        </div>
      </div>
      <div class="product-card" data-product="4">
        <div class="product-thumb" style="background:#1a1a2e">🖱️</div>
        <div class="product-info">
          <div class="product-name">Ergonomic Mouse</div>
          <div class="product-desc">7-button wireless, 25,000 DPI</div>
          <div class="product-price">$89</div>
        </div>
      </div>
      <div class="product-card" data-product="5">
        <div class="product-thumb" style="background:#2e2a1a">📷</div>
        <div class="product-info">
          <div class="product-name">Webcam Pro</div>
          <div class="product-desc">4K 60fps, auto-focus</div>
          <div class="product-price">$199</div>
        </div>
      </div>
      <div class="product-card" data-product="6">
        <div class="product-thumb" style="background:#1a2e2e">🎮</div>
        <div class="product-info">
          <div class="product-name">Gaming Controller</div>
          <div class="product-desc">Haptic feedback, USB-C</div>
          <div class="product-price">$79</div>
        </div>
      </div>
      <div class="product-card" data-product="7">
        <div class="product-thumb" style="background:#2e1a2e">🔊</div>
        <div class="product-info">
          <div class="product-name">Desktop Speakers</div>
          <div class="product-desc">2.1 stereo, 120W RMS</div>
          <div class="product-price">$249</div>
        </div>
      </div>
      <div class="product-card" data-product="8">
        <div class="product-thumb" style="background:#1a2e1a">⌨️</div>
        <div class="product-info">
          <div class="product-name">Portable SSD</div>
          <div class="product-desc">2TB, USB 3.2, 2000 MB/s</div>
          <div class="product-price">$189</div>
        </div>
      </div>
    </div>

    <div class="checkout-section">
      <h2>🛒 Quick Checkout</h2>
      <form id="checkout-form">
        <div class="form-group">
          <label for="checkout-name">Full Name</label>
          <input type="text" id="checkout-name" name="name" placeholder="Demo User" autocomplete="name">
        </div>
        <div class="form-group">
          <label for="checkout-email">Email Address</label>
          <input type="email" id="checkout-email" name="email" placeholder="demo@kitsune.ai" autocomplete="email">
        </div>
        <button type="submit" class="btn-submit">Complete Order →</button>
        <div id="checkout-result"></div>
      </form>
    </div>
  </main>

  <script>
    // Trackers that KitsuneEngine will block
    fetch('/api/doubleclick-tracker').catch(() => {});

    document.getElementById('checkout-form').addEventListener('submit', async (e) => {
      e.preventDefault();
      const data = new FormData(e.target);
      const res = await fetch('/checkout', {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify(Object.fromEntries(data.entries()))
      });
      const json = await res.json();
      const el = document.getElementById('checkout-result');
      el.style.display = 'block';
      el.style.color = json.success ? '#2ECC71' : '#E74C3C';
      el.textContent = json.success ? `✓ Order ${json.order_id} placed!` : 'Checkout failed';
    });
  </script>
</body>
</html>"#;

// ---------------------------------------------------------------------------
// Privacy report page
// ---------------------------------------------------------------------------

pub const PRIVACY_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Privacy Report — KitsuneEngine</title>
  <style>
    :root {
      --bg: #0D0D0F; --surface: #16161A; --elevated: #1E1E24;
      --border: #2A2A35; --accent: #6B5CE7; --accent-hover: #8B7CF8;
      --text: #F0EFF5; --muted: #9897A3; --success: #2ECC71; --error: #E74C3C;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { background: var(--bg); color: var(--text); font-family: 'Inter', system-ui, sans-serif; padding: 40px 20px; }
    .container { max-width: 800px; margin: 0 auto; }
    h1 { font-size: 28px; font-weight: 700; margin-bottom: 8px; }
    .subtitle { color: var(--muted); margin-bottom: 40px; }
    .report-table {
      background: var(--surface); border: 1px solid var(--border);
      border-radius: 16px; overflow: hidden; width: 100%;
    }
    .report-table thead { background: var(--elevated); }
    .report-table th, .report-table td { padding: 16px 24px; text-align: left; }
    .report-table th { font-size: 12px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--muted); font-weight: 600; }
    .report-table td { border-top: 1px solid var(--border); font-size: 15px; }
    .report-table tr:hover td { background: var(--elevated); }
    .value-good { color: var(--success); font-weight: 700; }
    .value-accent { color: var(--accent-hover); font-weight: 700; }
    .back-link { display: inline-block; margin-bottom: 24px; color: var(--muted); text-decoration: none; font-size: 14px; }
    .back-link:hover { color: var(--text); }
    .summary-banner {
      background: var(--surface); border: 1px solid var(--border);
      border-radius: 12px; padding: 24px; margin-bottom: 32px;
      display: grid; grid-template-columns: 60px 1fr; gap: 20px;
    }
    .summary-icon { font-size: 40px; }
    .summary-text h2 { font-size: 20px; font-weight: 700; margin-bottom: 4px; }
    .summary-text p { color: var(--muted); font-size: 14px; }
  </style>
</head>
<body>
  <div class="container">
    <a href="/" class="back-link">← Back to home</a>
    <h1>🔒 Privacy Report</h1>
    <p class="subtitle">Real-time summary of KitsuneEngine privacy enforcement on this session</p>

    <div class="summary-banner">
      <div class="summary-icon">🛡️</div>
      <div class="summary-text">
        <h2>Your privacy is protected</h2>
        <p>KitsuneEngine blocked all tracking attempts. No personal data was transmitted to third parties.</p>
      </div>
    </div>

    <table class="report-table">
      <thead>
        <tr>
          <th>Protection Layer</th>
          <th>Status</th>
          <th>Detail</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <td>Trackers Blocked</td>
          <td class="value-good" id="blocked-count">3</td>
          <td>Google Analytics, DoubleClick, custom tracker</td>
        </tr>
        <tr>
          <td>Referer Headers Stripped</td>
          <td class="value-good" id="stripped-count">1</td>
          <td>Outbound navigation referer removed</td>
        </tr>
        <tr>
          <td>DNT Headers Sent</td>
          <td class="value-good">Yes</td>
          <td>Do-Not-Track: 1 on all requests</td>
        </tr>
        <tr>
          <td>TLS Version</td>
          <td class="value-accent">TLS 1.3</td>
          <td>Minimum TLS 1.2 enforced; TLS 1.0/1.1 refused</td>
        </tr>
        <tr>
          <td>Fingerprinting</td>
          <td class="value-good">Mitigated</td>
          <td>Canvas noise, font enumeration blocked</td>
        </tr>
        <tr>
          <td>Third-Party Cookies</td>
          <td class="value-good">Blocked</td>
          <td>0 cross-site cookies permitted</td>
        </tr>
        <tr>
          <td>Vault Data Exposure</td>
          <td class="value-good">0 leaks</td>
          <td>All credentials remain in encrypted vault</td>
        </tr>
      </tbody>
    </table>
  </div>

  <script>
    // Read counts from query params if provided by kitsune-core
    const params = new URLSearchParams(location.search);
    if (params.has('blocked'))  document.getElementById('blocked-count').textContent  = params.get('blocked');
    if (params.has('stripped')) document.getElementById('stripped-count').textContent = params.get('stripped');
  </script>
</body>
</html>"#;
