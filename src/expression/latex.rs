use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::{boxed::Box, string::ToString, vec::Vec};

use nom::bytes::complete::{tag, take_while};
use nom::multi::separated_list0;
use nom::sequence::pair;
use nom::{
    branch::alt,
    bytes::complete::{take, take_while1},
    character::complete::{char, digit1, space0},
    combinator::map,
    multi::many0,
    sequence::{delimited, preceded, terminated, tuple},
    IResult,
};

use crate::expression::expression_tree::{Atom, Expression, Numeric};

use super::expression_tree::Escape;

pub fn parse(input: &str) -> Expression {
    parse_add_sub(input)
        .map_err(|_| "failed to parse")
        .unwrap()
        .1
}

fn parse_recursive(input: &str) -> IResult<&str, Expression> {
    alt((
        parse_parentheses,
        parse_frac,
        parse_vector,
        parse_matrix,
        parse_numeric,
        parse_function,
        parse_escape,
        parse_variable,
    ))(input)
}

fn parse_parentheses(input: &str) -> IResult<&str, Expression> {
    delimited(
        space0,
        alt((
            delimited(
                alt((tag("("), tag("\\left("))),
                parse_add_sub,
                alt((tag(")"), tag("\\right)"))),
            ),
            delimited(
                alt((tag("{"), tag("\\left{"))),
                parse_add_sub,
                alt((tag("}"), tag("\\right}"))),
            ),
        )),
        space0,
    )(input)
}

fn parse_numeric(input: &str) -> IResult<&str, Expression> {
    map(
        delimited(space0, take_while1(is_numeric_value), space0),
        parse_number,
    )(input)
}

fn is_numeric_value(c: char) -> bool {
    c.is_ascii_digit() || c == '.'
}

fn parse_number(input: &str) -> Expression {
    Expression::Atom(Atom::Numeric(match input.contains('.') {
        true => Numeric::Decimal(input.parse::<f32>().unwrap()),
        false => Numeric::Integer(input.parse::<i32>().unwrap()),
    }))
}

fn parse_function(input: &str) -> IResult<&str, Expression> {
    map(
        delimited(
            space0,
            tuple((
                preceded(
                    space0,
                    alt((
                        preceded(
                            tag("\\"),
                            pair(
                                take_while1(|c: char| c.is_alphabetic()),
                                take_while(|c: char| c.is_alphanumeric()),
                            ),
                        ),
                        pair(
                            take_while1(|c: char| c.is_alphabetic()),
                            take_while(|c: char| c.is_alphanumeric()),
                        ),
                    )),
                ),
                delimited(
                    alt((tag("("), tag("\\left("))),
                    pair(many0(terminated(parse_add_sub, char(','))), parse_add_sub),
                    alt((tag(")"), tag("\\right)"))),
                ),
            )),
            space0,
        ),
        |(name, arg_list)| Expression::Function {
            name: name.0.to_string() + name.1,
            args: arg_list
                .0
                .into_iter()
                .chain(vec![arg_list.1])
                .map(Box::new)
                .collect(),
        },
    )(input)
}

// TODO: fix vector and matrix parsing
fn parse_vector(input: &str) -> IResult<&str, Expression> {
    map(
        delimited(
            space0,
            delimited(
                char('<'),
                pair(many0(terminated(parse_add_sub, char(','))), parse_add_sub),
                char('>'),
            ),
            space0,
        ),
        |vector| Expression::Vector {
            size: vector.0.len() as u8 + 1,
            backing: vector
                .0
                .into_iter()
                .chain(vec![vector.1])
                .map(Box::new)
                .collect(),
        },
    )(input)
}

fn parse_matrix(input: &str) -> IResult<&str, Expression> {
    map(
        delimited(
            space0,
            delimited(
                char('['),
                separated_list0(char(';'), separated_list0(char(','), parse_add_sub)),
                char(']'),
            ),
            space0,
        ),
        |flatten_matrix| {
            let row_count = flatten_matrix.len() as u8;
            let col_count = flatten_matrix[0].len() as u8; // assuming every row has the same number of columns

            let backing = flatten_matrix.into_iter().flatten().map(Box::new).collect();
            Expression::Matrix {
                backing,
                shape: (row_count, col_count),
            }
        },
    )(input)
}

fn parse_escape(input: &str) -> IResult<&str, Expression> {
    map(
        delimited(
            space0,
            tuple((preceded(char('_'), take(1usize)), digit1)),
            space0,
        ),
        |(value, num): (&str, &str)| {
            Expression::Atom(Atom::Escape(
                match value.chars().next().unwrap() {
                    'A' => Escape::Atom,
                    'F' => Escape::Function,
                    'V' => Escape::Vector,
                    'M' => Escape::Matrix,
                    '*' => Escape::Everything,
                    _ => unreachable!(),
                },
                num.parse::<u8>().unwrap(),
            ))
        },
    )(input)
}

fn parse_variable(input: &str) -> IResult<&str, Expression> {
    map(delimited(space0, take(1usize), space0), |value: &str| {
        Expression::Atom(Atom::Variable(value.chars().next().unwrap()))
    })(input)
}

fn parse_unary(input: &str) -> IResult<&str, Expression> {
    alt((parse_unary_prefix, parse_unary_postfix, parse_exponents))(input)
}

fn parse_exponents(input: &str) -> IResult<&str, Expression> {
    let (input, num) = parse_recursive(input)?;
    let (input, ops) = many0(tuple((tag("^"), parse_exponents)))(input)?;
    Ok((input, fold_binary_operators(num, ops)))
}

fn parse_unary_prefix(input: &str) -> IResult<&str, Expression> {
    map(
        delimited(space0, tuple((tag("-"), parse_unary)), space0),
        parse_unary_prefix_op,
    )(input)
}

fn parse_unary_postfix(input: &str) -> IResult<&str, Expression> {
    map(
        delimited(space0, tuple((parse_exponents, tag("!"))), space0),
        parse_unary_postfix_op,
    )(input)
}

fn parse_frac(input: &str) -> IResult<&str, Expression> {
    map(
        delimited(
            space0,
            delimited(
                tag("\\frac"),
                tuple((
                    delimited(char('{'), parse_add_sub, char('}')),
                    delimited(char('{'), parse_add_sub, char('}')),
                )),
                space0,
            ),
            space0,
        ),
        |(num, den)| Expression::Divide(Box::new(num), Box::new(den)),
    )(input)
}

fn parse_mult_div_mod(input: &str) -> IResult<&str, Expression> {
    let (input, num) = parse_unary(input)?;
    let (input, ops) = many0(tuple((
        alt((tag("\\cdot"), tag("/"), tag("%"))),
        parse_unary,
    )))(input)?;
    Ok((input, fold_binary_operators(num, ops)))
}

fn parse_add_sub(input: &str) -> IResult<&str, Expression> {
    let (input, num) = parse_mult_div_mod(input)?;
    let (input, ops) = many0(tuple((alt((tag("+"), tag("-"))), parse_mult_div_mod)))(input)?;
    Ok((input, fold_binary_operators(num, ops)))
}

fn parse_unary_prefix_op(operator_pair: (&str, Expression)) -> Expression {
    let (operator, operand) = operator_pair;
    match operator {
        "-" => Expression::Negate(Box::new(operand)),
        _ => panic!("Invalid operator"),
    }
}

fn parse_unary_postfix_op(operator_pair: (Expression, &str)) -> Expression {
    let (operand, operator) = operator_pair;
    match operator {
        "!" => Expression::Factorial(Box::new(operand)),
        _ => panic!("Invalid operator"),
    }
}

fn fold_binary_operators(expr: Expression, ops: Vec<(&str, Expression)>) -> Expression {
    ops.into_iter()
        .fold(expr, |acc, val| parse_binary_op(val, acc))
}

fn parse_binary_op(operator_pair: (&str, Expression), expr1: Expression) -> Expression {
    let (operator, expr2) = operator_pair;
    match operator {
        "+" => Expression::Add(Box::new(expr1), Box::new(expr2)),
        "-" => Expression::Subtract(Box::new(expr1), Box::new(expr2)),
        "\\cdot" => Expression::Multiply(Box::new(expr1), Box::new(expr2)),
        "/" => Expression::Divide(Box::new(expr1), Box::new(expr2)),
        "^" => Expression::Power(Box::new(expr1), Box::new(expr2)),
        "%" => Expression::Modulus(Box::new(expr1), Box::new(expr2)),
        _ => panic!("Invalid operator"),
    }
}

pub fn latexify(expr: &Expression) -> String {
    match expr {
        Expression::Atom(a) => a.to_string(),

        Expression::Negate(e) => match **e {
            Expression::Atom(_) => "-".to_string() + &latexify(&e),
            _ => format!("-\\left({}\\right)", &latexify(&e)),
        },
        Expression::Factorial(e) => match **e {
            Expression::Atom(_) => format!("{}!", &latexify(&e)),
            _ => format!("\\left({}\\right)!", &latexify(&e)),
        },
        Expression::Percent(e) => match **e {
            Expression::Atom(_) => format!("{}%", &latexify(&e)),
            _ => format!("\\left({}\\right)%", &latexify(&e)),
        },

        Expression::Add(l, r) => format!("{}+{}", &latexify(&l), &latexify(&r)),
        Expression::Subtract(l, r) => format!("{}-{}", &latexify(&l), &latexify(&r)),
        Expression::Modulus(l, r) => format!("{}%{}", &latexify(&l), &latexify(&r)),

        Expression::Multiply(l, r) => {
            format!(
                "{}\\cdot{}",
                match **l {
                    Expression::Add(_, _)
                    | Expression::Subtract(_, _)
                    | Expression::Modulus(_, _) => format!("\\left({}\\right)", &latexify(&l)),
                    _ => format!("{}", &latexify(&l)),
                },
                match **r {
                    Expression::Add(_, _)
                    | Expression::Subtract(_, _)
                    | Expression::Modulus(_, _) => format!("\\left({}\\right)", &latexify(&r)),
                    _ => format!("{}", &latexify(&r)),
                }
            )
        }

        Expression::Divide(l, r) => {
            format!("\\frac{{{}}}{{{}}}", &latexify(&l), &latexify(&r))
        }

        Expression::Power(l, r) => {
            format!(
                "{}^{}",
                match **l {
                    Expression::Add(_, _)
                    | Expression::Subtract(_, _)
                    | Expression::Modulus(_, _)
                    | Expression::Multiply(_, _)
                    | Expression::Divide(_, _) => format!("\\left({}\\right)", &latexify(&l)),
                    _ => format!("{}", &latexify(&l)),
                },
                match **r {
                    Expression::Atom(_) => format!("{}", &latexify(&r)),
                    _ => format!("{{{}}}", &latexify(&r)),
                }
            )
        }

        Expression::Function { name, args } => {
            let mut out = format!("{}\\left(", name);
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out += ",";
                }
                out += &latexify(&arg);
            }
            format!("{}\\right)", out)
        }

        Expression::Vector {
            backing: vec,
            size: _,
        } => {
            format!("<");
            for (i, e) in vec.iter().enumerate() {
                if i > 0 {
                    format!(",");
                }
                format!("{}", e);
            }
            format!(">")
        }

        Expression::Matrix {
            backing: vec,
            shape: (rs, cs),
        } => {
            format!("[");
            for r in 0..*rs {
                if r > 0 {
                    format!(";");
                }
                for c in 0..*cs {
                    if c > 0 {
                        format!(",");
                    }
                    format!("{}", vec[(*cs * r + c) as usize]);
                }
            }
            format!("]")
        }
    }
}

mod tests {
    use super::*;

    #[test]
    fn complex_latex() {
        assert_eq!(
            parse("\\frac{5}{6}\\cdot5+\\left(4^{2+x}\\right)-1!+arc\\left(6\\right)"),
            Expression::Add(
                Box::new(Expression::Subtract(
                    Box::new(Expression::Add(
                        Box::new(Expression::Multiply(
                            Box::new(Expression::Divide(
                                Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(5)))),
                                Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(6))))
                            )),
                            Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(5))))
                        )),
                        Box::new(Expression::Power(
                            Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(4)))),
                            Box::new(Expression::Add(
                                Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(2)))),
                                Box::new(Expression::Atom(Atom::Variable('x')))
                            ))
                        ))
                    )),
                    Box::new(Expression::Factorial(Box::new(Expression::Atom(
                        Atom::Numeric(Numeric::Integer(1))
                    ))))
                )),
                Box::new(Expression::Function {
                    name: "arc".to_string(),
                    args: vec![Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(
                        6
                    ))))],
                })
            )
        )
    }

    #[test]
    fn recursive_latex() {
        assert_eq!(
            parse("5+(6+7)+8"),
            Expression::Add(
                Box::new(Expression::Add(
                    Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(5)))),
                    Box::new(Expression::Add(
                        Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(6)))),
                        Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(7))))
                    ))
                )),
                Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(8))))
            )
        )
    }

    #[test]
    fn complex_string_latex() {
        assert_eq!(
            "\\frac{5}{6}\\cdot5+4^{2+x}-1!+arc\\left(6\\right)",
            crate::expression::latex::latexify(&Expression::Add(
                Box::new(Expression::Subtract(
                    Box::new(Expression::Add(
                        Box::new(Expression::Multiply(
                            Box::new(Expression::Divide(
                                Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(5)))),
                                Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(6))))
                            )),
                            Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(5))))
                        )),
                        Box::new(Expression::Power(
                            Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(4)))),
                            Box::new(Expression::Add(
                                Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(2)))),
                                Box::new(Expression::Atom(Atom::Variable('x')))
                            ))
                        ))
                    )),
                    Box::new(Expression::Factorial(Box::new(Expression::Atom(
                        Atom::Numeric(Numeric::Integer(1))
                    ))))
                )),
                Box::new(Expression::Function {
                    name: "arc".to_string(),
                    args: vec![Box::new(Expression::Atom(Atom::Numeric(Numeric::Integer(
                        6
                    ))))],
                })
            ))
        )
    }
}
