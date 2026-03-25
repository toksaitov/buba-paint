-- Demo database for dashboard testing
-- Creates the bot's 6-table schema with realistic fixture data

CREATE TABLE tick_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    source TEXT NOT NULL,
    price REAL, bid REAL, ask REAL, bid_size REAL, ask_size REAL
);
CREATE TABLE markets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    market_id TEXT NOT NULL UNIQUE,
    question TEXT NOT NULL,
    condition_id TEXT NOT NULL,
    slug TEXT NOT NULL,
    up_token_id TEXT NOT NULL,
    down_token_id TEXT NOT NULL,
    start_time INTEGER NOT NULL,
    end_time INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'active'
);
CREATE TABLE signals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    strategy TEXT NOT NULL,
    direction TEXT NOT NULL,
    binance_price REAL,
    chainlink_price REAL,
    up_ask REAL, down_ask REAL,
    up_bid REAL, down_bid REAL,
    metadata TEXT
);
CREATE TABLE simulated_trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    market_id TEXT NOT NULL,
    strategy TEXT NOT NULL,
    side TEXT NOT NULL,
    token_id TEXT NOT NULL,
    entry_price REAL NOT NULL,
    size REAL NOT NULL,
    status TEXT NOT NULL DEFAULT 'open'
);
CREATE TABLE trade_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trade_id INTEGER NOT NULL UNIQUE,
    exit_price REAL,
    settlement_price REAL NOT NULL,
    pnl_0pct REAL NOT NULL,
    pnl_1pct REAL NOT NULL,
    pnl_2pct REAL NOT NULL,
    pnl_3pct REAL NOT NULL,
    resolved_at INTEGER NOT NULL
);
CREATE TABLE balance_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    event TEXT NOT NULL,
    trade_id INTEGER,
    amount REAL NOT NULL,
    balance REAL NOT NULL
);

-- Seed: 25 markets over ~2 hours
INSERT INTO markets VALUES (1,'mkt-001','BTC Up/Down 5m','cond-001','btc-updown-5m-1711065600','tok-up-1','tok-dn-1',1711065600000,1711065900000,'resolved');
INSERT INTO markets VALUES (2,'mkt-002','BTC Up/Down 5m','cond-002','btc-updown-5m-1711065900','tok-up-2','tok-dn-2',1711065900000,1711066200000,'resolved');
INSERT INTO markets VALUES (3,'mkt-003','BTC Up/Down 5m','cond-003','btc-updown-5m-1711066200','tok-up-3','tok-dn-3',1711066200000,1711066500000,'resolved');
INSERT INTO markets VALUES (4,'mkt-004','BTC Up/Down 5m','cond-004','btc-updown-5m-1711066500','tok-up-4','tok-dn-4',1711066500000,1711066800000,'resolved');
INSERT INTO markets VALUES (5,'mkt-005','BTC Up/Down 5m','cond-005','btc-updown-5m-1711066800','tok-up-5','tok-dn-5',1711066800000,1711067100000,'resolved');
INSERT INTO markets VALUES (6,'mkt-006','BTC Up/Down 5m','cond-006','btc-updown-5m-1711067100','tok-up-6','tok-dn-6',1711067100000,1711067400000,'resolved');
INSERT INTO markets VALUES (7,'mkt-007','BTC Up/Down 5m','cond-007','btc-updown-5m-1711067400','tok-up-7','tok-dn-7',1711067400000,1711067700000,'resolved');
INSERT INTO markets VALUES (8,'mkt-008','BTC Up/Down 5m','cond-008','btc-updown-5m-1711067700','tok-up-8','tok-dn-8',1711067700000,1711068000000,'resolved');
INSERT INTO markets VALUES (9,'mkt-009','BTC Up/Down 5m','cond-009','btc-updown-5m-1711068000','tok-up-9','tok-dn-9',1711068000000,1711068300000,'active');

-- Seed: balance log (init + 18 settlements -> nice equity curve)
INSERT INTO balance_log VALUES (1, 1711065600000,'init',NULL,0.0,200.00);
INSERT INTO balance_log VALUES (2, 1711065650000,'settlement',1,22.50,222.50);
INSERT INTO balance_log VALUES (3, 1711065950000,'settlement',2,-15.00,207.50);
INSERT INTO balance_log VALUES (4, 1711066000000,'settlement',3,35.00,242.50);
INSERT INTO balance_log VALUES (5, 1711066250000,'settlement',4,18.00,260.50);
INSERT INTO balance_log VALUES (6, 1711066300000,'settlement',5,-12.00,248.50);
INSERT INTO balance_log VALUES (7, 1711066550000,'settlement',6,42.00,290.50);
INSERT INTO balance_log VALUES (8, 1711066600000,'settlement',7,28.00,318.50);
INSERT INTO balance_log VALUES (9, 1711066850000,'settlement',8,-20.00,298.50);
INSERT INTO balance_log VALUES (10,1711066900000,'settlement',9,55.00,353.50);
INSERT INTO balance_log VALUES (11,1711067150000,'settlement',10,-18.00,335.50);
INSERT INTO balance_log VALUES (12,1711067200000,'settlement',11,32.00,367.50);
INSERT INTO balance_log VALUES (13,1711067450000,'settlement',12,45.00,412.50);
INSERT INTO balance_log VALUES (14,1711067500000,'settlement',13,-25.00,387.50);
INSERT INTO balance_log VALUES (15,1711067750000,'settlement',14,60.00,447.50);
INSERT INTO balance_log VALUES (16,1711067800000,'settlement',15,38.00,485.50);
INSERT INTO balance_log VALUES (17,1711068050000,'settlement',16,-30.00,455.50);
INSERT INTO balance_log VALUES (18,1711068100000,'settlement',17,70.00,525.50);
INSERT INTO balance_log VALUES (19,1711068150000,'settlement',18,15.00,540.50);

-- Seed: 20 trades (18 settled + 2 open)
INSERT INTO simulated_trades VALUES (1, 1711065610000,'mkt-001','latency-arb','UP','tok-up-1',0.4500,50.00,'closed');
INSERT INTO simulated_trades VALUES (2, 1711065910000,'mkt-002','latency-arb','DOWN','tok-dn-2',0.5200,40.00,'closed');
INSERT INTO simulated_trades VALUES (3, 1711065960000,'mkt-003','spread-capture','UP','tok-up-3',0.4800,60.00,'closed');
INSERT INTO simulated_trades VALUES (4, 1711066210000,'mkt-003','latency-arb','UP','tok-up-3',0.4300,45.00,'closed');
INSERT INTO simulated_trades VALUES (5, 1711066260000,'mkt-004','latency-arb','DOWN','tok-dn-4',0.5100,35.00,'closed');
INSERT INTO simulated_trades VALUES (6, 1711066510000,'mkt-004','spread-capture','UP','tok-up-4',0.4600,70.00,'closed');
INSERT INTO simulated_trades VALUES (7, 1711066560000,'mkt-005','latency-arb','UP','tok-up-5',0.4200,55.00,'closed');
INSERT INTO simulated_trades VALUES (8, 1711066810000,'mkt-005','latency-arb','DOWN','tok-dn-5',0.5300,45.00,'closed');
INSERT INTO simulated_trades VALUES (9, 1711066860000,'mkt-006','latency-arb','UP','tok-up-6',0.4100,80.00,'closed');
INSERT INTO simulated_trades VALUES (10,1711067110000,'mkt-006','spread-capture','DOWN','tok-dn-6',0.4900,50.00,'closed');
INSERT INTO simulated_trades VALUES (11,1711067160000,'mkt-007','latency-arb','UP','tok-up-7',0.4400,60.00,'closed');
INSERT INTO simulated_trades VALUES (12,1711067410000,'mkt-007','latency-arb','UP','tok-up-7',0.4000,75.00,'closed');
INSERT INTO simulated_trades VALUES (13,1711067460000,'mkt-008','latency-arb','DOWN','tok-dn-8',0.5400,55.00,'closed');
INSERT INTO simulated_trades VALUES (14,1711067710000,'mkt-008','spread-capture','UP','tok-up-8',0.4700,90.00,'closed');
INSERT INTO simulated_trades VALUES (15,1711067760000,'mkt-008','latency-arb','UP','tok-up-8',0.4300,65.00,'closed');
INSERT INTO simulated_trades VALUES (16,1711068010000,'mkt-009','latency-arb','DOWN','tok-dn-9',0.5200,50.00,'closed');
INSERT INTO simulated_trades VALUES (17,1711068060000,'mkt-009','latency-arb','UP','tok-up-9',0.4100,85.00,'closed');
INSERT INTO simulated_trades VALUES (18,1711068110000,'mkt-009','spread-capture','DOWN','tok-dn-9',0.4800,40.00,'closed');
-- 2 open trades
INSERT INTO simulated_trades VALUES (19,1711068200000,'mkt-009','latency-arb','UP','tok-up-9',0.4500,70.00,'open');
INSERT INTO simulated_trades VALUES (20,1711068220000,'mkt-009','spread-capture','DOWN','tok-dn-9',0.5000,55.00,'open');

-- Seed: trade results for the 18 closed trades
INSERT INTO trade_results VALUES (1, 1,1.0,1.0,  22.50, 22.05, 21.60, 21.15, 1711065650000);
INSERT INTO trade_results VALUES (2, 2,0.0,0.0, -15.00,-15.30,-15.60,-15.90, 1711065950000);
INSERT INTO trade_results VALUES (3, 3,1.0,1.0,  35.00, 34.30, 33.60, 32.90, 1711066000000);
INSERT INTO trade_results VALUES (4, 4,1.0,1.0,  18.00, 17.64, 17.28, 16.92, 1711066250000);
INSERT INTO trade_results VALUES (5, 5,0.0,0.0, -12.00,-12.24,-12.48,-12.72, 1711066300000);
INSERT INTO trade_results VALUES (6, 6,1.0,1.0,  42.00, 41.16, 40.32, 39.48, 1711066550000);
INSERT INTO trade_results VALUES (7, 7,1.0,1.0,  28.00, 27.44, 26.88, 26.32, 1711066600000);
INSERT INTO trade_results VALUES (8, 8,0.0,0.0, -20.00,-20.40,-20.80,-21.20, 1711066850000);
INSERT INTO trade_results VALUES (9, 9,1.0,1.0,  55.00, 53.90, 52.80, 51.70, 1711066900000);
INSERT INTO trade_results VALUES (10,10,0.0,0.0,-18.00,-18.36,-18.72,-19.08, 1711067150000);
INSERT INTO trade_results VALUES (11,11,1.0,1.0, 32.00, 31.36, 30.72, 30.08, 1711067200000);
INSERT INTO trade_results VALUES (12,12,1.0,1.0, 45.00, 44.10, 43.20, 42.30, 1711067450000);
INSERT INTO trade_results VALUES (13,13,0.0,0.0,-25.00,-25.50,-26.00,-26.50, 1711067500000);
INSERT INTO trade_results VALUES (14,14,1.0,1.0, 60.00, 58.80, 57.60, 56.40, 1711067750000);
INSERT INTO trade_results VALUES (15,15,1.0,1.0, 38.00, 37.24, 36.48, 35.72, 1711067800000);
INSERT INTO trade_results VALUES (16,16,0.0,0.0,-30.00,-30.60,-31.20,-31.80, 1711068050000);
INSERT INTO trade_results VALUES (17,17,1.0,1.0, 70.00, 68.60, 67.20, 65.80, 1711068100000);
INSERT INTO trade_results VALUES (18,18,1.0,1.0, 15.00, 14.70, 14.40, 14.10, 1711068150000);

-- Seed: 25 signals
INSERT INTO signals VALUES (1, 1711065605000,'latency-arb','UP',   84250.00,84252.00,0.45,0.55,0.44,0.54,'{"momentum":0.0028}');
INSERT INTO signals VALUES (2, 1711065905000,'latency-arb','DOWN', 84180.00,84178.00,0.48,0.52,0.47,0.51,'{"momentum":-0.0015}');
INSERT INTO signals VALUES (3, 1711065955000,'spread-capture','UP',84190.00,84191.00,0.48,0.50,0.47,0.49,'{"spread":0.98}');
INSERT INTO signals VALUES (4, 1711066205000,'latency-arb','UP',   84320.00,84322.00,0.43,0.57,0.42,0.56,'{"momentum":0.0032}');
INSERT INTO signals VALUES (5, 1711066255000,'latency-arb','DOWN', 84280.00,84277.00,0.49,0.51,0.48,0.50,'{"momentum":-0.0011}');
INSERT INTO signals VALUES (6, 1711066505000,'spread-capture','UP',84350.00,84351.00,0.46,0.52,0.45,0.51,'{"spread":0.98}');
INSERT INTO signals VALUES (7, 1711066555000,'latency-arb','UP',   84410.00,84413.00,0.42,0.58,0.41,0.57,'{"momentum":0.0035}');
INSERT INTO signals VALUES (8, 1711066805000,'latency-arb','DOWN', 84380.00,84376.00,0.47,0.53,0.46,0.52,'{"momentum":-0.0018}');
INSERT INTO signals VALUES (9, 1711066855000,'latency-arb','UP',   84450.00,84453.00,0.41,0.59,0.40,0.58,'{"momentum":0.0041}');
INSERT INTO signals VALUES (10,1711067105000,'spread-capture','DOWN',84420.00,84419.00,0.49,0.49,0.48,0.48,'{"spread":0.98}');
INSERT INTO signals VALUES (11,1711067155000,'latency-arb','UP',   84500.00,84503.00,0.44,0.56,0.43,0.55,'{"momentum":0.0022}');
INSERT INTO signals VALUES (12,1711067405000,'latency-arb','UP',   84560.00,84564.00,0.40,0.60,0.39,0.59,'{"momentum":0.0038}');
INSERT INTO signals VALUES (13,1711067455000,'latency-arb','DOWN', 84520.00,84517.00,0.46,0.54,0.45,0.53,'{"momentum":-0.0014}');
INSERT INTO signals VALUES (14,1711067705000,'spread-capture','UP',84580.00,84581.00,0.47,0.51,0.46,0.50,'{"spread":0.98}');
INSERT INTO signals VALUES (15,1711067755000,'latency-arb','UP',   84620.00,84624.00,0.43,0.57,0.42,0.56,'{"momentum":0.0029}');
INSERT INTO signals VALUES (16,1711068005000,'latency-arb','DOWN', 84590.00,84586.00,0.48,0.52,0.47,0.51,'{"momentum":-0.0016}');
INSERT INTO signals VALUES (17,1711068055000,'latency-arb','UP',   84650.00,84654.00,0.41,0.59,0.40,0.58,'{"momentum":0.0044}');
INSERT INTO signals VALUES (18,1711068105000,'spread-capture','DOWN',84630.00,84629.00,0.48,0.50,0.47,0.49,'{"spread":0.98}');
INSERT INTO signals VALUES (19,1711068195000,'latency-arb','UP',   84680.00,84684.00,0.45,0.55,0.44,0.54,'{"momentum":0.0025}');
INSERT INTO signals VALUES (20,1711068215000,'spread-capture','DOWN',84670.00,84669.00,0.50,0.48,0.49,0.47,'{"spread":0.98}');

-- Tick data (just bookend entries for uptime calc)
INSERT INTO tick_data VALUES (1, 1711065600000,'binance',84200.00,NULL,NULL,NULL,NULL);
INSERT INTO tick_data VALUES (2, 1711068300000,'binance',84700.00,NULL,NULL,NULL,NULL);
