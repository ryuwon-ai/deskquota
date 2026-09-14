include!("probe.rs");

fn main() {
    let mut rows=Vec::new();
    for (i,number) in ["nan","+inf","-inf"].iter().enumerate() {
        let f=Fixture::new(&format!("nonfinite-detail-{i}"));let p=f.file("settings.toml");
        let original=format!("value = {number}\nkeep = true\n");write(&p,original.as_bytes());
        let t=tx(&f,&p,Format::Toml,&[(&["value"],json!(1))]);
        let shown=preview(&t).expect("current reproduction expects preview acceptance");
        apply(&t,&shown.hash).expect("current reproduction expects apply success");
        let first=restore(&t.journal_path).expect_err("current reproduction expects restore failure").to_string();
        let summary=llmgw::config_patch::inspect_journal(&t.journal_path).unwrap();
        let backup=summary.resources[0].backup_refs.first().unwrap();
        let backup_exact=fs::read(backup).unwrap()==original.as_bytes();
        let current=fs::read_to_string(&p).unwrap().parse::<toml_edit::DocumentMut>().unwrap();
        let again=restore(&t.journal_path).is_err();
        rows.push(json!({"original_literal":number,"preview_accepted":true,"apply_succeeded":true,"restore_failed":true,
            "restore_reports_null_representation_error":first.contains("TOML has no safe null value representation"),
            "journal_status":format!("{:?}",summary.status),"original_before_image_retained":backup_exact,
            "current_value_still_replacement":current["value"].as_integer()==Some(1),"unrelated_keep_preserved":current["keep"].as_bool()==Some(true),
            "retry_restore_failed":again}));
    }
    println!("{}",serde_json::to_string_pretty(&json!({"finding":"Q3","reproduced_cases":rows,"owned_fixtures_cleaned":true})).unwrap());
}
