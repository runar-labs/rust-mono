// Test for the service and action macros
//
// This test demonstrates how to use the service and action macros
// to create a simple service with actions.

use std::collections::HashMap;

use runar_common::types::ArcValue;
use runar_services::sqlite_cipher::{
    ColumnDefinition, DataType, Params, Schema, SqlQuery, SqliteService, SqliteServiceConfig,
    TableDefinition, Value,
};
use serde::{Deserialize, Serialize}; // For User and MyData structs

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct User {
    id: Option<i64>,
    name: String,
    age: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct MyData {
    id: i32,
    text_field: String,
    number_field: i32,
    boolean_field: bool,
    float_field: f64,
    vector_field: Vec<i32>,
    map_field: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use runar_node::config::LogLevel;
    use runar_node::config::LoggingConfig;
    use runar_node::Node;
    use runar_node::NodeConfig;

    #[tokio::test]
    async fn test_insert_with_encryption() {
        //set log to debug
        let logging_config = LoggingConfig::new().with_default_level(LogLevel::Debug);

        // Create a node with a test network ID
        let config =
            NodeConfig::new("test-node", "test_network").with_logging_config(logging_config);
        let mut node = Node::new(config).await.unwrap();

        let schema = Schema {
            tables: vec![TableDefinition {
                name: "users".to_string(),
                columns: vec![
                    ColumnDefinition {
                        name: "id".to_string(),
                        data_type: DataType::Integer,
                        primary_key: true,
                        autoincrement: true,
                        not_null: true,
                    },
                    ColumnDefinition {
                        name: "name".to_string(),
                        data_type: DataType::Text,
                        primary_key: false,
                        autoincrement: false,
                        not_null: true,
                    },
                    ColumnDefinition {
                        name: "age".to_string(),
                        data_type: DataType::Integer,
                        primary_key: false,
                        autoincrement: false,
                        not_null: false, // Age can be null for this test example
                    },
                ],
            }],
            indexes: vec![], // Ensure all fields of Schema are initialized
        };

        // Create raw key bytes directly for testing (32 bytes for AES-256)
        let key_bytes: Vec<u8> = (0..32).map(|i| i as u8).collect();
        println!("Using symmetric key: {}", hex::encode(&key_bytes));

        // We don't need the key manager for this test since we're using raw key bytes directly

        // Create the SQLite service with the raw key bytes directly
        let service_name = "users_db".to_string();
        let sqlite_config = SqliteServiceConfig {
            db_path: ":memory:".to_string(), // Use in-memory database for tests
            schema,
            key_id: None, // No key_id needed since we're using raw key bytes directly
            symmetric_key: Some(key_bytes), // Pass the raw key bytes directly
        };

        let service = SqliteService::new(&service_name, sqlite_config).unwrap();

        // Add the service to the node
        node.add_service(service).await.unwrap();

        // Start the node to initialize all services
        node.start().await.unwrap();

        // Test SQLite INSERT operation
        let insert_params = Params::new()
            .with_value(Value::Text("Test User From SqlQuery".to_string()))
            .with_value(Value::Integer(33));
        let insert_query =
            SqlQuery::new("INSERT INTO users (name, age) VALUES (?, ?)").with_params(insert_params);
        let arc_insert_query = ArcValue::from_struct(insert_query.clone());

        let insert_response: i64 = node
            .request("users_db/execute_query", Some(arc_insert_query))
            .await
            .unwrap();
        let affected_rows = insert_response;
        assert_eq!(affected_rows, 1, "INSERT should affect 1 row");

        // Test SQLite SELECT operation
        let select_params =
            Params::new().with_value(Value::Text("Test User From SqlQuery".to_string()));
        let select_query = SqlQuery::new("SELECT id, name, age FROM users WHERE name = ?")
            .with_params(select_params);
        let arc_select_query = ArcValue::from_struct(select_query.clone());

        let select_response: Vec<ArcValue> = node
            .request("users_db/execute_query", Some(arc_select_query))
            .await
            .unwrap();
        let result_list = select_response;
        assert_eq!(result_list.len(), 1, "SELECT should return one user");

        let mut user_map_av = result_list[0].clone();
        let user_map = user_map_av
            .as_map_ref::<String, ArcValue>()
            .expect("User data should be a map");

        let mut name_av = user_map
            .get("name")
            .expect("User map should have 'name'")
            .clone();
        assert_eq!(
            name_av.as_type::<String>().unwrap(),
            "Test User From SqlQuery"
        );

        let mut age_av = user_map
            .get("age")
            .expect("User map should have 'age'")
            .clone();
        assert_eq!(age_av.as_type::<i64>().unwrap(), 33);
    }
}
