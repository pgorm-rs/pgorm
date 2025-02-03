//! Translating the SQL AST into engine-specific SQL statements.

use crate::*;

mod query_builder;

pub use self::query_builder::*;

#[derive(Debug, PartialEq)]
pub enum Oper {
    UnOper(UnOper),
    BinOper(BinOper),
}

impl From<UnOper> for Oper {
    fn from(value: UnOper) -> Self {
        Oper::UnOper(value)
    }
}

impl From<BinOper> for Oper {
    fn from(value: BinOper) -> Self {
        Oper::BinOper(value)
    }
}

impl Oper {
    pub(crate) fn is_logical(&self) -> bool {
        matches!(
            self,
            Oper::UnOper(UnOper::Not) | Oper::BinOper(BinOper::And) | Oper::BinOper(BinOper::Or)
        )
    }

    pub(crate) fn is_between(&self) -> bool {
        matches!(
            self,
            Oper::BinOper(BinOper::Between) | Oper::BinOper(BinOper::NotBetween)
        )
    }

    pub(crate) fn is_like(&self) -> bool {
        matches!(
            self,
            Oper::BinOper(BinOper::Like) | Oper::BinOper(BinOper::NotLike)
        )
    }

    pub(crate) fn is_in(&self) -> bool {
        matches!(
            self,
            Oper::BinOper(BinOper::In) | Oper::BinOper(BinOper::NotIn)
        )
    }

    pub(crate) fn is_is(&self) -> bool {
        matches!(
            self,
            Oper::BinOper(BinOper::Is) | Oper::BinOper(BinOper::IsNot)
        )
    }

    pub(crate) fn is_shift(&self) -> bool {
        matches!(
            self,
            Oper::BinOper(BinOper::LShift) | Oper::BinOper(BinOper::RShift)
        )
    }

    pub(crate) fn is_arithmetic(&self) -> bool {
        match self {
            Oper::BinOper(b) => {
                matches!(
                    b,
                    BinOper::Mul | BinOper::Div | BinOper::Mod | BinOper::Add | BinOper::Sub
                )
            }
            _ => false,
        }
    }

    pub(crate) fn is_comparison(&self) -> bool {
        match self {
            Oper::BinOper(b) => {
                matches!(
                    b,
                    BinOper::SmallerThan
                        | BinOper::SmallerThanOrEqual
                        | BinOper::Equal
                        | BinOper::GreaterThanOrEqual
                        | BinOper::GreaterThan
                        | BinOper::NotEqual
                )
            }
            _ => false,
        }
    }
}
