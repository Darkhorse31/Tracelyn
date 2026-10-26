#[cfg(test)]
mod tests {
    use super::super::aggregate::{TrafficAggregator, HEATMAP_MINUTES};
    use super::super::config::{parse_apache_config, parse_nginx_config, ParsedWebConfig};
    use super::super::detect::detect_from_probe_outputs;
    use super::super::parse::{
        parse_access_line, parse_apache_timing_line, LogFormatKind,
    };
    use super::super::route::{normalize_path, RouteRule};
    use super::super::WebServerProbe;
    use super::super::route::RouteNormalizer;

    const NGINX_COMBINED: &str =
        r#"192.168.1.10 - - [23/Oct/2026:10:00:01 +0000] "GET /api/users/42/orders HTTP/1.1" 200 512 "-" "curl/8.0""#;

    const NGINX_TIMING: &str =
        r#"192.168.1.10 - - [23/Oct/2026:10:00:02 +0000] "GET /api/users/99 HTTP/1.1" 200 256 "-" "curl/8.0" 0.042 0.038"#;

    const APACHE_COMBINED: &str =
        r#"10.0.0.5 - - [23/Oct/2026:10:00:03 +0000] "POST /checkout HTTP/1.1" 500 0 "https://shop" "Mozilla/5.0""#;

    #[test]
    fn parse_nginx_combined() {
        let e = parse_access_line(NGINX_COMBINED, LogFormatKind::Combined).unwrap();
        assert_eq!(e.remote_addr, "192.168.1.10");
        assert_eq!(e.method, "GET");
        assert_eq!(e.path, "/api/users/42/orders");
        assert_eq!(e.status, 200);
        assert_eq!(e.bytes_sent, 512);
        assert!(e.request_time_secs.is_none());
    }

    #[test]
    fn parse_nginx_custom_timing() {
        let e = parse_access_line(NGINX_TIMING, LogFormatKind::Custom).unwrap();
        assert_eq!(e.path, "/api/users/99");
        assert_eq!(e.status, 200);
        let rt = e.request_time_secs.unwrap();
        assert!((rt - 0.042).abs() < 0.001);
        let ut = e.upstream_time_secs.unwrap();
        assert!((ut - 0.038).abs() < 0.001);
    }

    #[test]
    fn parse_apache_combined() {
        let e = parse_access_line(APACHE_COMBINED, LogFormatKind::Combined).unwrap();
        assert_eq!(e.status, 500);
        assert_eq!(e.path, "/checkout");
    }

    #[test]
    fn parse_apache_percent_d() {
        let line = r#"10.0.0.5 - - [23/Oct/2026:10:00:04 +0000] "GET /health HTTP/1.1" 200 12 "-" "-" 125000"#;
        let e = parse_apache_timing_line(line).unwrap();
        let ms = e.request_time_secs.unwrap() * 1000.0;
        assert!((ms - 125.0).abs() < 0.1);
        let via = parse_access_line(line, LogFormatKind::ApacheTiming).unwrap();
        assert!((via.request_time_secs.unwrap() - 0.125).abs() < 0.001);
    }

    #[test]
    fn detect_nginx_from_ps() {
        let probe = WebServerProbe {
            ps_output: "root  1  nginx: master process /usr/sbin/nginx\n".into(),
            nginx_version: Some("nginx version: nginx/1.24.0".into()),
            apache_version: None,
            nginx_config: None,
            apache_config: None,
        };
        let d = detect_from_probe_outputs(&probe).unwrap().unwrap();
        assert_eq!(d.kind, super::super::config::WebServerKind::Nginx);
    }

    #[test]
    fn detect_apache_from_ps() {
        let probe = WebServerProbe {
            ps_output: "www-data  2  /usr/sbin/apache2 -k start\n".into(),
            nginx_version: None,
            apache_version: Some("Server version: Apache/2.4.57".into()),
            nginx_config: None,
            apache_config: None,
        };
        let d = detect_from_probe_outputs(&probe).unwrap().unwrap();
        assert_eq!(d.kind, super::super::config::WebServerKind::Apache);
    }

    #[test]
    fn nginx_config_logs_and_missing_timing() {
        let cfg = include_str!("../../tests/fixtures/nginx.conf");
        let parsed = parse_nginx_config(cfg);
        let ParsedWebConfig::Nginx(info) = parsed else {
            panic!("expected nginx");
        };
        assert!(info.access_log_paths.iter().any(|p| p.contains("access.log")));
        assert!(!info.request_timing.present);
        assert!(info.request_timing.suggested_line.is_some());
        assert!(info
            .request_timing
            .suggested_line
            .unwrap()
            .contains("$request_time"));
    }

    #[test]
    fn nginx_config_with_timing() {
        let cfg = include_str!("../../tests/fixtures/nginx-timing.conf");
        let parsed = parse_nginx_config(cfg);
        let ParsedWebConfig::Nginx(info) = parsed else {
            panic!("expected nginx");
        };
        assert!(info.request_timing.present);
        assert!(info.request_timing.suggested_line.is_none());
    }

    #[test]
    fn apache_config_missing_timing() {
        let cfg = include_str!("../../tests/fixtures/apache.conf");
        let parsed = parse_apache_config(cfg);
        let ParsedWebConfig::Apache(info) = parsed else {
            panic!("expected apache");
        };
        assert!(!info.request_timing.present);
        assert!(info
            .request_timing
            .suggested_line
            .unwrap()
            .contains("%D"));
    }

    #[test]
    fn route_normalization() {
        let rules = vec![RouteRule {
            pattern: r"^/api/v\d+".to_string(),
            replacement: "/api/v:ver".to_string(),
        }];
        let p = normalize_path(
            "/api/v2/users/12345/profile/8f14e45f-ceea-467a-9bcf-2bc95f89c297",
            &rules,
        );
        assert!(p.contains("/:id"));
        assert!(!p.contains("12345"));
        assert!(!p.contains("8f14e45f"));
    }

    #[test]
    fn aggregator_rates_and_endpoints() {
        let mut agg = TrafficAggregator::new(60, RouteNormalizer::default());
        let ts = 1_000_000u64;
        for i in 0..100 {
            let path = format!("/api/item/{}/view", i);
            let line = format!(
                r#"1.2.3.4 - - [23/Oct/2026:10:00:00 +0000] "GET {} HTTP/1.1" 200 1000 "-" "-" 0.05 0.04"#,
                path
            );
            let entry = parse_access_line(&line, LogFormatKind::Custom).unwrap();
            agg.ingest(&entry, ts);
        }
        let snap = agg.traffic_snapshot();
        assert!(snap.req_per_sec > 0.0);
        let ep = agg.endpoint_snapshot();
        assert!(ep.endpoints.iter().any(|e| e.route == "/api/item/:id/view"));
        let normalized = ep
            .endpoints
            .iter()
            .find(|e| e.route == "/api/item/:id/view")
            .unwrap();
        assert!(normalized.p50_ms.is_some());
        assert_eq!(normalized.request_count, 100);
    }

    #[test]
    fn heatmap_drops_buckets_outside_window() {
        let mut agg = TrafficAggregator::new(60, RouteNormalizer::default());
        let line = r#"1.2.3.4 - - [23/Oct/2026:10:00:00 +0000] "GET /a HTTP/1.1" 200 10 "-" "-" 0.05 0.04"#;
        let entry = parse_access_line(line, LogFormatKind::Custom).unwrap();
        let start = 60 * 1_000_000u64;
        agg.ingest(&entry, start);
        agg.ingest(&entry, start + 60 * (HEATMAP_MINUTES - 1));
        assert_eq!(agg.endpoint_snapshot().heatmap.len(), 2);
        agg.ingest(&entry, start + 60 * HEATMAP_MINUTES);
        let buckets: Vec<u64> = agg
            .endpoint_snapshot()
            .heatmap
            .iter()
            .map(|c| c.bucket_secs)
            .collect();
        assert_eq!(buckets.len(), 2);
        assert!(buckets.iter().all(|b| *b > start));
    }
}
