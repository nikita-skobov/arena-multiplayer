use ensko::ensko;

ensko!(
    ensko_cfg = {
        generate_entrypoint = main
    };
    import = { @inline { dynamotable = ::ensko_aws::dynamodb::table::DdbTable } };

    const mytable = dynamotable {
        table_name = "mygametable2025"
        pkey_name = { shared::PKEY }
        region = "us-east-1"
        skey_name = { Some(shared::SKEY.to_string()) }
    };
);
