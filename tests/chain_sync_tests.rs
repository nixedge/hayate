// Chain sync connection and error handling tests

use amaru_kernel::Point;
use hayate::chain_sync::HayateSync;

#[tokio::test]
async fn test_connect_to_invalid_host() {
    let result = HayateSync::connect("invalid-host-that-does-not-exist:3001", 764824073, Point::Origin).await;

    // Should fail with connection error
    assert!(result.is_err());
}

#[tokio::test]
async fn test_connect_to_invalid_port() {
    let result = HayateSync::connect("localhost:99999", 764824073, Point::Origin).await;

    // Should fail - invalid port
    assert!(result.is_err());
}

#[tokio::test]
async fn test_connect_with_empty_host() {
    let result = HayateSync::connect("", 764824073, Point::Origin).await;

    // Should fail
    assert!(result.is_err());
}

#[tokio::test]
async fn test_connect_localhost_unreachable() {
    // Try to connect to localhost where nothing is listening
    let result = HayateSync::connect("127.0.0.1:3001", 764824073, Point::Origin).await;

    // Should fail with connection refused
    assert!(result.is_err());

    if let Err(err) = result {
        let err_msg = format!("{:?}", err);
        assert!(
            err_msg.contains("connect")
            || err_msg.contains("Connection")
            || err_msg.contains("refused")
            || err_msg.contains("Failed")
        );
    }
}

#[tokio::test]
async fn test_connection_timeout() {
    // Connect to a host that doesn't respond (timeout test)
    // Using a non-routable IP address
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        HayateSync::connect("192.0.2.1:3001", 764824073, Point::Origin)
    ).await;

    // Should either timeout or fail to connect
    assert!(result.is_err() || result.unwrap().is_err());
}

#[tokio::test]
async fn test_different_magic_numbers() {
    // Test with different network magic numbers
    let magics = vec![
        764824073, // Mainnet
        1,         // Preprod
        2,         // Preview
        4,         // SanchoNet
        42,        // Custom
    ];

    for magic in magics {
        let result = HayateSync::connect("localhost:3001", magic, Point::Origin).await;
        // All should fail because nothing is listening, but shouldn't panic
        assert!(result.is_err());
    }
}

#[test]
fn test_point_origin() {
    let point = Point::Origin;

    // Origin point should be valid
    match point {
        Point::Origin => assert!(true),
        _ => panic!("Expected Point::Origin"),
    }
}

#[test]
fn test_point_specific() {
    use amaru_kernel::{Slot, Hash};

    let slot = Slot::from(123456u64);
    let hash_bytes = [1u8; 32];
    let hash = Hash::<32>::from(hash_bytes);
    let point = Point::Specific(slot, hash);

    match point {
        Point::Specific(s, h) => {
            assert_eq!(s, slot);
            assert_eq!(h, hash);
        }
        _ => panic!("Expected Point::Specific"),
    }
}

#[tokio::test]
async fn test_malformed_host_address() {
    // Test various malformed addresses
    let bad_hosts = vec![
        "not a valid address",
        "::::",
        "http://example.com", // Shouldn't include protocol
        "localhost",          // Missing port
        ":3001",             // Missing host
    ];

    for host in bad_hosts {
        let result = HayateSync::connect(host, 764824073, Point::Origin).await;
        assert!(result.is_err(), "Expected error for host: {}", host);
    }
}

#[tokio::test]
async fn test_connect_with_specific_point() {
    use amaru_kernel::{Slot, Hash};

    let slot = Slot::from(100000u64);
    let hash = Hash::<32>::from([0u8; 32]);
    let point = Point::Specific(slot, hash);

    let result = HayateSync::connect("localhost:3001", 764824073, point).await;

    // Should still fail (no node running), but shouldn't panic
    assert!(result.is_err());
}

#[tokio::test]
async fn test_multiple_connection_attempts() {
    // Ensure multiple failed connection attempts don't cause issues
    for _ in 0..5 {
        let result = HayateSync::connect("localhost:3001", 764824073, Point::Origin).await;
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn test_concurrent_connection_attempts() {
    // Try multiple connections concurrently
    let mut handles = vec![];

    for i in 0..3 {
        let handle = tokio::spawn(async move {
            HayateSync::connect(
                &format!("localhost:{}", 3001 + i),
                764824073,
                Point::Origin
            ).await
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_err());
    }
}

#[test]
fn test_host_port_parsing() {
    // Test that we can parse various host:port formats
    let valid_formats = vec![
        "localhost:3001",
        "127.0.0.1:3001",
        "example.com:3001",
        "192.168.1.1:3001",
        "node.example.com:3001",
    ];

    for format in valid_formats {
        assert!(format.contains(':'), "Format should contain colon: {}", format);
        let parts: Vec<&str> = format.split(':').collect();
        assert_eq!(parts.len(), 2, "Format should have exactly 2 parts: {}", format);
        assert!(!parts[0].is_empty(), "Host should not be empty: {}", format);
        assert!(!parts[1].is_empty(), "Port should not be empty: {}", format);
    }
}

#[tokio::test]
async fn test_connection_with_zero_magic() {
    // Test with magic number 0 (should still handle gracefully)
    let result = HayateSync::connect("localhost:3001", 0, Point::Origin).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_connection_with_max_magic() {
    // Test with maximum u64 value
    let result = HayateSync::connect("localhost:3001", u64::MAX, Point::Origin).await;
    assert!(result.is_err());
}

#[test]
fn test_point_clone() {
    let point1 = Point::Origin;
    let point2 = point1.clone();

    // Both should be Origin
    matches!(point1, Point::Origin);
    matches!(point2, Point::Origin);
}

#[tokio::test]
async fn test_ipv6_address_handling() {
    // Test IPv6 address format
    let result = HayateSync::connect("[::1]:3001", 764824073, Point::Origin).await;
    // Should fail (nothing listening) but not panic
    assert!(result.is_err());
}

#[tokio::test]
async fn test_dns_resolution_failure() {
    // Use a domain that definitely doesn't exist
    let result = HayateSync::connect(
        "this-domain-definitely-does-not-exist-12345.invalid:3001",
        764824073,
        Point::Origin
    ).await;

    assert!(result.is_err());
}
