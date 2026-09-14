use llmgw::autostart::{Platform,RegistrationSpec,login_auth_availability};
use llmgw::config::Auth;
use std::{path::Path,fs};
fn emit(platform:Platform,name:&str)->RegistrationSpec {
 let s=RegistrationSpec::new(platform,Path::new("/fixture 한글 & $100%/llmgw"),Path::new("/fixture 한글 & $100%/config \\\"<>&.toml")).unwrap();
 fs::write(Path::new(env!("REVIEW_OUT")).join(name),s.definition()).unwrap();s
}
#[test] fn mac_template_literal_and_no_restart(){let s=emit(Platform::Macos,"generated-mac.plist");assert!(s.definition().contains("<key>KeepAlive</key>\n<false/>"));assert!(s.definition().contains("<string>run</string>"));assert!(s.definition().contains("$100%"));}
#[test] fn linux_template_literal_expansion_disabled(){let s=emit(Platform::Linux,"generated-linux.service");assert!(s.definition().contains("ExecStart=:\""));assert!(s.definition().contains("$100%%"));assert!(s.definition().contains("Restart=no"));}
#[test] fn windows_template_limits_and_foreground(){let s=emit(Platform::Windows,"generated-windows.xml");assert!(s.definition().contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));assert!(s.definition().contains("<LogonType>InteractiveToken</LogonType>"));assert!(s.definition().contains("<RunLevel>LeastPrivilege</RunLevel>"));}
#[test] fn windows_logon_trigger_is_scoped_to_current_user(){let s=emit(Platform::Windows,"generated-windows-user.xml");let trigger=s.definition().split("<LogonTrigger>").nth(1).unwrap().split("</LogonTrigger>").next().unwrap();assert!(trigger.contains("<UserId>"),"current-user LogonTrigger must contain its reviewed UserId/SID");}
#[test] fn unobserved_login_env_is_unknown(){let a=login_auth_availability(&Auth::Env{header:"authorization".into(),name:"REVIEW_SYNTHETIC_AUTH".into()});assert_eq!(a.to_string(),"unknown","terminal cannot observe next-login environment; neither presence nor absence is proof");}
