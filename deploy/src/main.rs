use ensko::ensko;

const INLINE_POLICY: &'static str = r#"
{
    "Version": "2012-10-17",
    "Statement": [
        {"Effect": "Allow", "Action": "dynamodb:*", "Resource": "resource_arn_here"},
        {"Effect": "Allow", "Action": "lambda:InvokeFunction", "Resource": "arn:aws:lambda:::function:mygamething"}
    ]
}"#;


fn create_inline_policy(table_arn: &str) -> String {
    INLINE_POLICY.replace("resource_arn_here", table_arn).replace("\n", "")
}

fn get_environment_vars() -> String {
    r#"{"hello": "world"}"#.to_string()
}

ensko!(
    ensko_cfg = {
        generate_entrypoint = deploy
    };
    import = {
        @inline {
            dynamotable = ::ensko_aws::dynamodb::table::DdbTable
            lambdafn = ::ensko_aws::lambda::function::LambdaFunction
            lambdaurl = ::ensko_aws::lambda::url::LambdaFunctionUrl
            iamrole = ::ensko_aws::iam::role::IamRole
        }
    };

    const mytable = dynamotable {
        table_name = "mygametable2025"
        pkey_name = { shared::PKEY }
        region = "us-east-1"
        skey_name = { Some(shared::SKEY.to_string()) }
    };

    const lambdarole = iamrole {
        role_name = "lambda-game-role"
        assume_role_policy = { ::ensko_aws::iam::role::LAMBDA_ASSUME_ROLE_POLICY }
        inline_policy = { crate::create_inline_policy(&mytable.table_arn) }
    } on [mytable];

    const server = lambdafn {
        function_name = "mygamething"
        region = "us-east-1"
        role_arn = lambdarole.role_arn
        memory_size_mb = 128u32
        timeout_secs = 60u32
        crate_name = "server"
        environment = { Some(crate::get_environment_vars()) }
    } on [lambdarole];

    const serverurl = lambdaurl {
        function_name = server.function_name
    } on [server];
);

fn main() {
    match deploy() {
        Ok(o) => {
            println!("Function URL: {:?}", o.serverurl.item.and_then(|x| Some(x.function_url)));
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
