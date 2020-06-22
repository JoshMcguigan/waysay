#[derive(Clone)]
pub struct Args {
    pub message: String,
    pub buttons: Vec<ArgButton>,
    pub message_type: String,
    pub detailed_message: bool,
}

#[derive(Clone)]
pub struct ArgButton {
    pub text: String,
    pub action: String,
}

pub fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut message = None;
    let mut message_type = None;
    let mut buttons = vec![];
    let mut detailed_message = false;

    // skip the binary name
    let mut args = args.skip(1);

    loop {
        match args.next().as_deref() {
            Some("-m") | Some("--message") => {
                let message_arg = args.next();

                if message_arg.is_some() {
                    message = message_arg;
                } else {
                    return Err("missing required arg message (-m/--message)".into());
                }
            }
            Some("-t") | Some("--type") => {
                let message_type_arg = args.next();

                if message_type_arg.is_some() {
                    message_type = message_type_arg;
                } else {
                    return Err("missing required arg type (-t/--type)".into());
                }
            }
            Some("-l") | Some("--detailed-message") => {
                detailed_message = true;
            }
            // For now handle both -b and -B the same
            Some("-b") | Some("--button") | Some("-B") | Some("--button-no-terminal") => {
                let text = args.next();
                let action = args.next();

                match (text, action) {
                    (Some(text), Some(action)) => buttons.push(ArgButton { text, action }),
                    (None, _) => return Err("button missing text".into()),
                    (Some(_), None) => return Err("button missing action".into()),
                }
            }
            Some(arg) => return Err(format!("invalid arg '{}'", arg)),
            None => break,
        }
    }

    if let Some(message) = message {
        Ok(Args {
            message,
            buttons,
            message_type: message_type.unwrap_or_else(|| "error".into()),
            detailed_message,
        })
    } else {
        Err("missing required arg message (-m/--message)".into())
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn no_args() {
        let input = vec!["waysay".into()];

        assert_eq!(
            "missing required arg message (-m/--message)",
            parse(input.into_iter()).err().unwrap(),
        );
    }

    #[test]
    fn unsupported_arg() {
        let input = vec!["waysay".into(), "--not-a-real-thing".into()];

        assert_eq!(
            "invalid arg '--not-a-real-thing'",
            parse(input.into_iter()).err().unwrap(),
        );
    }

    #[test]
    fn message_short_flag() {
        let input = vec!["waysay".into(), "-m".into(), "hello from waysay".into()];

        let args = parse(input.into_iter()).unwrap();
        assert_eq!("hello from waysay", args.message,);
    }

    #[test]
    fn message_long_flag() {
        let input = vec![
            "waysay".into(),
            "--message".into(),
            "hello from waysay".into(),
        ];

        let args = parse(input.into_iter()).unwrap();
        assert_eq!("hello from waysay", args.message,);
    }
}
