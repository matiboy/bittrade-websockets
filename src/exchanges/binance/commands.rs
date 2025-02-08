use crate::control::control::ControlCommand;

pub enum Command {
    AddPair(String),
    RemovePair(String),
    AddKey(String, String),
}

impl TryFrom<ControlCommand> for Command {
    type Error = ();
    fn try_from(value: ControlCommand) -> Result<Self, Self::Error> {
        match value {
            ControlCommand::AddPair(_, pair) => Ok(Command::AddPair(pair)),
            ControlCommand::RemovePair(_, pair) => Ok(Command::RemovePair(pair)),
            _ => Err(()),
        }
    }
}
#[cfg(test)]
mod tests {
    use crate::exchanges::exchange::ExchangeName;

    use super::*;

    #[test]
    fn test_try_from_add_pair() {
        let control_command = ControlCommand::AddPair(ExchangeName::Binance, "BTCUSD".to_string());
        let command = Command::try_from(control_command);
        assert!(matches!(command, Ok(Command::AddPair(pair)) if pair == "BTCUSD"));
    }

    #[test]
    fn test_try_from_remove_pair() {
        let control_command = ControlCommand::RemovePair(ExchangeName::Binance, "BTCUSD".to_string());
        let command = Command::try_from(control_command);
        assert!(matches!(command, Ok(Command::RemovePair(pair)) if pair == "BTCUSD"));
    }

    #[test]
    fn test_try_from_invalid_command() {
        let control_command = ControlCommand::AddKey("".to_owned(), "".to_owned());
        let command = Command::try_from(control_command);
        assert!(matches!(command, Err(())));
    }
}
