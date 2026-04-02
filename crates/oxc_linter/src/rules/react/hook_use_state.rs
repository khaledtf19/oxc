use crate::{
    AstNode,
    context::LintContext,
    rule::{DefaultRuleConfig, Rule},
    utils::is_react_function_call,
};
use oxc_ast::{AstKind, ast::BindingPattern};
use oxc_diagnostics::OxcDiagnostic;
use oxc_macros::declare_oxc_lint;
use oxc_span::{CompactStr, GetSpan, Span};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn require_to_destruct(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("useState call is not destructured into value + setter pair")
        .with_help(
            "Destructure useState call into value + setter pair \
            follow the [thing, setThing] naming convention",
        )
        .with_label(span)
}

fn follow_naming_convention(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("useState call does not follow the [thing, setThing] naming convention")
        .with_help("Follow the [thing, setThing] naming convention")
        .with_label(span)
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct HookUseStateConfig {
    /// ### allowDestructuredState
    /// When true the rule will ignore the name of the destructured value.
    allow_destructured_state: bool,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
pub struct HookUseState(HookUseStateConfig);

// See <https://github.com/oxc-project/oxc/issues/6050> for documentation details.
declare_oxc_lint!(
    /// ### What it does
    ///
    /// Ensure destructuring and symmetric naming of useState hook value and setter variables.
    ///
    /// ### Why is this bad?
    ///
    /// This rule checks whether the value and setter variables destructured from a React.useState() call are named symmetrically
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```jsx
    /// import React from 'react';
    /// export default function useColor() {
    ///  // useState call is not destructured into value + setter pair
    ///  const useStateResult = React.useState();
    ///  return useStateResult;
    /// }
    /// ```
    ///```jsx
    /// import React from 'react';
    /// export default function useColor() {
    ///  // useState call is destructured into value + setter pair, but identifier
    ///  // names do not follow the [thing, setThing] naming convention
    ///  const [color, updateColor] = React.useState();
    ///  return [color, updateColor];
    /// }
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```jsx
    /// import React from 'react';
    ///export default function useColor() {
    ///  // useState call is destructured into value + setter pair whose identifiers
    ///  // follow the [thing, setThing] naming convention
    ///  const [color, setColor] = React.useState();
    ///  return [color, setColor];
    ///}
    /// ```
    HookUseState,
    react,
    style,
    pending,
    config = HookUseState,
);

impl Rule for HookUseState {
    fn from_configuration(value: serde_json::Value) -> Result<Self, serde_json::error::Error> {
        serde_json::from_value::<DefaultRuleConfig<Self>>(value).map(DefaultRuleConfig::into_inner)
    }

    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let is_use_state = is_react_function_call(call, "useState");
        if !is_use_state {
            return;
        }

        let parent = ctx.nodes().parent_node(call.node_id());

        let var = match parent.kind() {
            AstKind::VariableDeclarator(var) => var,
            AstKind::ReturnStatement(_) => return,
            _ => {
                ctx.diagnostic(require_to_destruct(call.span()));
                return;
            }
        };

        let BindingPattern::ArrayPattern(array_pattern) = &var.id else {
            ctx.diagnostic(require_to_destruct(var.span()));
            return;
        };

        if array_pattern.elements.len() != 2 || array_pattern.rest.is_some() {
            ctx.diagnostic(require_to_destruct(array_pattern.span()));
            return;
        }

        let Some(value_node) = &array_pattern.elements[0] else {
            ctx.diagnostic(require_to_destruct(array_pattern.span()));
            return;
        };
        let Some(setter_node) = &array_pattern.elements[1] else {
            ctx.diagnostic(require_to_destruct(array_pattern.span()));
            return;
        };

        if is_destructured_pattern(setter_node) {
            ctx.diagnostic(require_to_destruct(array_pattern.span()));
            return;
        }

        if is_destructured_pattern(value_node) {
            if self.0.allow_destructured_state {
                return;
            }
            ctx.diagnostic(require_to_destruct(array_pattern.span()));
            return;
        }

        let Some(value_variable_name) = value_node.get_identifier_name() else {
            ctx.diagnostic(require_to_destruct(array_pattern.span()));
            return;
        };
        let value = value_variable_name.to_compact_str();

        let Some(setter_variable_name) = setter_node.get_identifier_name() else {
            ctx.diagnostic(require_to_destruct(array_pattern.span()));
            return;
        };
        let setter = setter_variable_name.to_compact_str();

        let Some((lowercase_prefix, suffix)) = split_leading_lowercase(&value) else {
            ctx.diagnostic(follow_naming_convention(array_pattern.span()));
            return;
        };
        let valid_setter_names = get_expected_setter_vars(&lowercase_prefix, &suffix);

        if valid_setter_names.contains(&setter) {
            return;
        }

        ctx.diagnostic(follow_naming_convention(array_pattern.span()));
    }
}
fn get_expected_setter_vars(first: &CompactStr, second: &CompactStr) -> [CompactStr; 2] {
    let first_capitalized = capitalize(first);

    let one = CompactStr::new(&format!("set{}{}", first_capitalized, second));
    let two = CompactStr::new(&format!("set{}{}", first.to_uppercase(), second));

    [one, two]
}
fn capitalize(s: &str) -> CompactStr {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    if let Some(c) = chars.next() {
        for u in c.to_uppercase() {
            out.push(u);
        }
    }
    out.push_str(chars.as_str());
    CompactStr::from(out)
}
fn split_leading_lowercase(s: &CompactStr) -> Option<(CompactStr, CompactStr)> {
    let s_str = s.as_str();
    let split_at = s_str.chars().take_while(|c| c.is_ascii_lowercase()).map(char::len_utf8).sum();

    if split_at == 0 {
        return None;
    }

    Some((CompactStr::new(&s_str[..split_at]), CompactStr::new(&s_str[split_at..])))
}

fn is_destructured_pattern(pattern: &BindingPattern) -> bool {
    matches!(pattern, BindingPattern::ObjectPattern(_) | BindingPattern::ArrayPattern(_))
}

#[test]
fn test() {
    use crate::tester::Tester;

    let pass = vec![
        (
            r"
            import React from 'react';
            export default function useColor() {
                return React.useState();
            }",
            None,
        ),
        (
            r"
            import React from 'react';
            export default function useColor() {
                const [color, setColor] = React.useState();
                return [color, setColor];
            }",
            None,
        ),
        (
            r"
            import React from 'react';
            export default function useColor() {
                return React.useState('');
            }",
            None,
        ),
        (
            r"
            import React from 'react';
            export default function useReq() {
                const [{ res, error }, setRes] = React.useState({ res: '', error: '' });
                return { res, error, setRes };
            }",
            Some(serde_json::json!([{"allowDestructuredState": true }])),
        ),
        (
            r"
            import React from 'react';
            export default function useReq() {
                const [[ res, error ], setRes] = React.useState(['', '']);
                return { res, error, setRes };
            }",
            Some(serde_json::json!([{"allowDestructuredState": true }])),
        ),
    ];

    let fail = vec![
        (
            r"
            import React from 'react';
            export default function useColor() {
                const color = React.useState();
                return color;
            }",
            None,
        ),
        (
            r"
            import React from 'react';
            export default function useColor() {
                const [color, updateColor] = React.useState();
                return [color, updateColor];
            }",
            None,
        ),
        (
            r"
                import React from 'react';
                export default function useColor() {
                  const [RGB , setRGB] = React.useState();
                  return [RGB, setRGB];
                }",
            None,
        ),
        (
            r"
            import React from 'react';
            export default function useReq() {
                const [{ res, error }, setRes] = React.useState({ res: '', error: '' });
                return { res, error, setRes };
            }",
            None,
        ),
        (
            r"
            import React from 'react';
            export default function useReq() {
                const [[ res, error ], setRes] = React.useState(['', '']);
                return { res, error, setRes };
            }",
            None,
        ),
        (
            r"
            import React from 'react';
            export default function useReq() {
                const [res, {}] = React.useState('');
                return [res, {}];
            }",
            None,
        ),
    ];

    Tester::new(HookUseState::NAME, HookUseState::PLUGIN, pass, fail).test_and_snapshot();
}
