use serde::de::{self, Deserializer};

pub fn deserialize_f64_from_string<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrF64;

    impl<'de> de::Visitor<'de> for StringOrF64 {
        type Value = f64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or number representing a float")
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value
                .parse::<f64>()
                .map_err(|_| E::invalid_value(de::Unexpected::Str(value), &self))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value as f64)
        }
    }

    deserializer.deserialize_any(StringOrF64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::IntoDeserializer;
    use serde_json::json;

    #[test]
    fn test_deserialize_f64_from_float() {
        let deserializer: de::value::F64Deserializer<serde::de::value::Error> = 42.5.into_deserializer();
        let result: Result<f64, _> = deserialize_f64_from_string(deserializer);
        assert_eq!(result.unwrap(), 42.5);
    }

    #[test]
    fn test_deserialize_f64_from_integer() {
        let deserializer: de::value::I32Deserializer<serde::de::value::Error> = 10.into_deserializer();
        let result = deserialize_f64_from_string(deserializer);
        assert_eq!(result.unwrap(), 10.0);
    }

    #[test]
    fn test_deserialize_f64_from_negative_integer() {
        let deserializer: de::value::I32Deserializer<serde::de::value::Error> = (-15).into_deserializer();
        let result = deserialize_f64_from_string(deserializer);
        assert_eq!(result.unwrap(), -15.0);
    }

    #[test]
    fn test_deserialize_f64_from_string() {
        let deserializer: de::value::StrDeserializer<serde::de::value::Error>  = "3.14159".into_deserializer();
        let result = deserialize_f64_from_string(deserializer);
        assert_eq!(result.unwrap(), 3.14159);
    }

    #[test]
    fn test_deserialize_f64_from_string_without_decimal_places() {
        let deserializer: de::value::StrDeserializer<serde::de::value::Error> = "100".into_deserializer();
        let result = deserialize_f64_from_string(deserializer);
        assert_eq!(result.unwrap(), 100.0);
    }

    #[test]
    fn test_deserialize_f64_from_invalid_string() {
        let deserializer: de::value::StrDeserializer<serde::de::value::Error> = "not_a_number".into_deserializer();
        let result = deserialize_f64_from_string(deserializer);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_f64_from_empty_string() {
        let deserializer: de::value::StrDeserializer<serde::de::value::Error> = "".into_deserializer();
        let result = deserialize_f64_from_string(deserializer);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_f64_from_json_number_via_value() {
        let json_value = json!(12.34);
        let deserializer = json_value.into_deserializer();
        let result = deserialize_f64_from_string(deserializer);
        assert_eq!(result.unwrap(), 12.34);
    }

    #[test]
    fn test_deserialize_f64_from_json_string() {
        let json_value = json!("56.78");
        let deserializer = json_value.into_deserializer();
        let result = deserialize_f64_from_string(deserializer);
        assert_eq!(result.unwrap(), 56.78);
    }

    #[test]
    fn test_deserialize_f64_from_json_integer() {
        let json_value = json!(25);
        let deserializer = json_value.into_deserializer();
        let result = deserialize_f64_from_string(deserializer);
        assert_eq!(result.unwrap(), 25.0);
    }
}
