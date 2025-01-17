#![cfg(test)]

use crate::{
    codegen::{
        json::{
            call_deserialize_fn, DeserializeJsonFn, JsonColumn, JsonColumnParseSpec,
            JsonDeserConfig, JsonSerConfig, SerializeFn,
        },
        Codegen, CodegenConfig,
    },
    ir::{ColumnType, RowLayoutBuilder, RowLayoutCache},
    row::{row_from_literal, UninitRow},
    utils::{self, HashMap},
};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use std::mem::transmute;

#[test]
fn deserialize_json_smoke() {
    utils::test_logger();

    let layout_cache = RowLayoutCache::new();
    let layout = layout_cache.add(
        RowLayoutBuilder::new()
            .with_column(ColumnType::String, false)
            .with_column(ColumnType::String, true)
            .with_column(ColumnType::I64, false)
            .with_column(ColumnType::I64, true)
            .with_column(ColumnType::F64, false)
            .with_column(ColumnType::F64, true)
            .with_column(ColumnType::Date, false)
            .with_column(ColumnType::Time, false)
            .build(),
    );

    let mut codegen = Codegen::new(layout_cache, CodegenConfig::debug());

    let deserialize = JsonDeserConfig {
        layout,
        mappings: {
            [
                JsonColumn::normal("/foo"),
                JsonColumn::normal("/bar"),
                JsonColumn::normal("/baz"),
                JsonColumn::normal("/bing"),
                JsonColumn::normal("/bop"),
                JsonColumn::normal("/boop"),
                JsonColumn::datetime("/bang", "%F"),
                JsonColumn::datetime("/bazinga", "%H:%M:%S%.f"),
            ]
            .into_iter()
            .enumerate()
            .collect()
        },
    };
    let serialize = JsonSerConfig {
        layout,
        mappings: {
            [
                JsonColumn::normal("foo"),
                JsonColumn::normal("bar"),
                JsonColumn::normal("baz"),
                JsonColumn::normal("bing"),
                JsonColumn::normal("bop"),
                JsonColumn::normal("boop"),
                JsonColumn::datetime("bang", "%F"),
                JsonColumn::datetime("bazinga", "%H:%M:%S%.f"),
            ]
            .into_iter()
            .enumerate()
            .collect()
        },
    };

    let deserialize_json = codegen.deserialize_json(&deserialize);
    let serialize_json = codegen.serialize_json(&serialize);
    let vtable = codegen.vtable_for(layout);

    let json_snippets = &[
        r#"{ "foo": "foo data string", "bar": "bar data string", "baz": 10, "bing": 100, "bop": 96.542, "boop": -1245.53, "bang": "2023-09-20", "bazinga": "12:04:43.5436" }"#,
        r#"{ "foo": "second foo data string", "bar": null, "baz": -10000, "bing": null, "bop": -0.0, "boop": null, "bang": "1999-09-09", "bazinga": "00:00:00.0000" }"#,
        r#"{ "baz": -32, "bar": null, "foo": "woah, now we switched the field orderings", "bop": 0.3, "bang": "2000-01-01", "bazinga": "12:59:59.9999" }"#,
        r#"{ "baz": 0, "bar": null, "foo": "", "bop": "NaN", "boop": "Inf", "bang": "2098-11-28", "bazinga": "01:02:03.4567" }"#,
    ];

    #[rustfmt::skip]
    let expected = &[
        row!["foo data string", ?"bar data string", 10i64, ?100i64, 96.542f64, ?-1245.53f64, NaiveDate::from_ymd_opt(2023, 9, 20).unwrap(), NaiveTime::parse_from_str("12:04:43.5436", "%H:%M:%S%.f").unwrap()],
        row!["second foo data string", null, -10000i64, null, -0.0, null, NaiveDate::from_ymd_opt(1999, 9, 9).unwrap(), NaiveTime::parse_from_str("00:00:00.0000", "%H:%M:%S%.f").unwrap()],
        row!["woah, now we switched the field orderings", null, -32i64, null, 0.3, null, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(), NaiveTime::parse_from_str("12:59:59.9999", "%H:%M:%S%.f").unwrap()],
        row!["", null, 0i64, null, f64::NAN, ?f64::INFINITY, NaiveDate::from_ymd_opt(2098, 11, 28).unwrap(), NaiveTime::parse_from_str("01:02:03.4567", "%H:%M:%S%.f").unwrap()],
    ];

    let (jit, layout_cache) = codegen.finalize_definitions();
    let vtable = Box::into_raw(Box::new(vtable.marshalled(&jit)));

    {
        let (deserialize_json, serialize_json) = unsafe {
            (
                transmute::<_, DeserializeJsonFn>(jit.get_finalized_function(deserialize_json)),
                transmute::<_, SerializeFn>(jit.get_finalized_function(serialize_json)),
            )
        };

        let mut serialize_buffer = Vec::new();
        for (&json, expected) in json_snippets.iter().zip(expected) {
            let json_value = serde_json::from_str(json).unwrap();
            let mut uninit = UninitRow::new(unsafe { &*vtable });

            let row = unsafe {
                call_deserialize_fn(deserialize_json, uninit.as_mut_ptr(), &json_value).unwrap();
                uninit.assume_init()
            };

            let expected =
                unsafe { row_from_literal(expected, &*vtable, &layout_cache.layout_of(layout)) };
            assert_eq!(
                row,
                expected,
                "input json: {json:?}\nrow value for {}: {row:?}",
                layout_cache.row_layout(layout),
            );

            unsafe { serialize_json(row.as_ptr(), &mut serialize_buffer) }
            // assert_eq!(
            //     json_value,
            //     serde_json::from_str::<serde_json::Value>(&serialize_buffer).unwrap(),
            // );
            println!("{}", std::str::from_utf8(&serialize_buffer).unwrap());
            serialize_buffer.clear();
        }
    }

    unsafe {
        drop(Box::from_raw(vtable));
        jit.free_memory();
    }
}

#[test]
#[should_panic = "an error occurred while parsing the key \"/FOO\""]
fn deserialize_invalid_json() {
    utils::test_logger();

    let layout_cache = RowLayoutCache::new();
    let layout = layout_cache.add(
        RowLayoutBuilder::new()
            .with_column(ColumnType::String, false)
            .build(),
    );

    let mut codegen = Codegen::new(layout_cache, CodegenConfig::debug());

    let deserialize = JsonDeserConfig {
        layout,
        mappings: {
            let mut mappings = HashMap::default();
            mappings.insert(0, JsonColumn::normal("/foo"));
            mappings
        },
    };

    let deserialize_json = codegen.deserialize_json(&deserialize);
    let vtable = codegen.vtable_for(layout);

    let (jit, _layout_cache) = codegen.finalize_definitions();
    let vtable = Box::into_raw(Box::new(vtable.marshalled(&jit)));

    {
        let deserialize_json = unsafe {
            transmute::<_, DeserializeJsonFn>(jit.get_finalized_function(deserialize_json))
        };

        let json_value = serde_json::from_str(r#"{ "foo": 10 }"#).unwrap();
        let mut uninit = UninitRow::new(unsafe { &*vtable });

        unsafe {
            match call_deserialize_fn(deserialize_json, uninit.as_mut_ptr(), &json_value) {
                // This shouldn't ever be ok
                Ok(()) => {}
                Err(error) => panic!("{error}"),
            }
        }
    }

    unsafe {
        drop(Box::from_raw(vtable));
        jit.free_memory();
    }
}

#[test]
fn deserialize_parsing() {
    utils::test_logger();

    let layout_cache = RowLayoutCache::new();
    let layout = layout_cache.add(
        RowLayoutBuilder::new()
            .with_column(ColumnType::Date, false)
            .with_column(ColumnType::Timestamp, false)
            .with_column(ColumnType::Timestamp, false)
            .build(),
    );

    let mut codegen = Codegen::new(layout_cache, CodegenConfig::debug());

    let deserialize = JsonDeserConfig {
        layout,
        mappings: {
            let columns = [
                JsonColumn::new("/foo", JsonColumnParseSpec::DateFromDays),
                JsonColumn::new("/bar", JsonColumnParseSpec::TimeFromMillis),
                JsonColumn::new("/baz", JsonColumnParseSpec::TimeFromMicros),
            ];

            let mut mappings = HashMap::with_capacity_and_hasher(columns.len(), Default::default());
            for (idx, column) in columns.into_iter().enumerate() {
                mappings.insert(idx, column);
            }

            mappings
        },
    };

    let deserialize_json = codegen.deserialize_json(&deserialize);
    let vtable = codegen.vtable_for(layout);

    let (jit, layout_cache) = codegen.finalize_definitions();
    let vtable = Box::into_raw(Box::new(vtable.marshalled(&jit)));

    {
        let deserialize_json = unsafe {
            transmute::<_, DeserializeJsonFn>(jit.get_finalized_function(deserialize_json))
        };

        // 2098-11-28 = 47083 days
        // 2022-06-30 10:35:00 = 1656585300000 millis
        // 2022-06-30 10:35:00 = 1656585300000000 micros
        let json_value = serde_json::from_str(
            r#"{ "foo": 47083, "bar": 1656585300000, "baz": 1656585300000000 }"#,
        )
        .unwrap();
        let mut uninit = UninitRow::new(unsafe { &*vtable });

        unsafe {
            match call_deserialize_fn(deserialize_json, uninit.as_mut_ptr(), &json_value) {
                // This shouldn't ever be ok
                Ok(()) => {}
                Err(error) => panic!("{error}"),
            }
        }

        let row = unsafe { uninit.assume_init() };

        let expected = row![
            NaiveDate::parse_from_str("2098-11-28", "%F").unwrap(),
            NaiveDateTime::parse_from_str("2022-06-30 10:35:00", "%F %T").unwrap(),
            NaiveDateTime::parse_from_str("2022-06-30 10:35:00", "%F %T").unwrap(),
        ];
        let expected =
            unsafe { row_from_literal(&expected, &*vtable, &layout_cache.layout_of(layout)) };

        assert_eq!(row, expected);
    }

    unsafe {
        drop(Box::from_raw(vtable));
        jit.free_memory();
    }
}
