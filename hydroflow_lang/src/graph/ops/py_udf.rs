use proc_macro2::Literal;
use quote::quote_spanned;

use super::{
    FlowProperties, FlowPropertyVal, OperatorCategory, OperatorConstraints, OperatorInstance,
    OperatorWriteOutput, WriteContextArgs, RANGE_0, RANGE_1,
};

/// > Arguments: First, the source code for a python module, second, the name of a unary function
/// > defined within the module source code.
///
/// **Requires the "python" feature to be enabled.**
///
/// An operator which allows you to run a python udf. Input arguments must be a stream of tuples
/// whose items implement [`IntoPy`](https://docs.rs/pyo3/latest/pyo3/conversion/trait.IntoPy.html).
/// See the [relevant pyo3 docs here](https://pyo3.rs/latest/conversions/tables#mapping-of-rust-types-to-python-types).
///
/// Output items are of type `PyResult<Py<PyAny>>`. Rust native types can be extracted using
/// `.extract()`, see the [relevant pyo3 docs here](https://pyo3.rs/latest/conversions/traits#extract-and-the-frompyobject-trait)
/// or the examples below.
///
/// ```hydroflow
/// source_iter(0..10)
///     -> map(|x| (x,))
///     -> py_udf(r#"
/// def fib(n):
///     if n < 2:
///         return n
///     else:
///         return fib(n - 2) + fib(n - 1)
///     "#, "fib")
///     -> map(|x: PyResult<Py<PyAny>>| Python::with_gil(|py| {
///         usize::extract(x.unwrap().as_ref(py)).unwrap()
///     }))
///     -> assert([0, 1, 1, 2, 3, 5, 8, 13, 21, 34]);
/// ```
///
/// ```hydroflow
/// source_iter([(5,1)])
/// -> py_udf(r#"
/// def add(a, b):
///     return a + b
///             "#, "add")
///             -> map(|x: PyResult<Py<PyAny>>| Python::with_gil(|py| {
///                 usize::extract(x.unwrap().as_ref(py)).unwrap()
///             }))
///             -> assert([6]);
/// ```
pub const PY_UDF: OperatorConstraints = OperatorConstraints {
    name: "py_udf",
    categories: &[OperatorCategory::Map],
    hard_range_inn: RANGE_1,
    soft_range_inn: RANGE_1,
    hard_range_out: RANGE_1,
    soft_range_out: RANGE_1,
    num_args: 2,
    persistence_args: RANGE_0,
    type_args: RANGE_0,
    is_external_input: false,
    ports_inn: None,
    ports_out: None,
    properties: FlowProperties {
        deterministic: FlowPropertyVal::DependsOnArgs,
        monotonic: FlowPropertyVal::DependsOnArgs,
        inconsistency_tainted: false,
    },
    input_delaytype_fn: |_| None,
    write_fn: |wc @ &WriteContextArgs {
                   root,
                   op_span,
                   context,
                   hydroflow,
                   ident,
                   inputs,
                   outputs,
                   is_pull,
                   op_name,
                   op_inst: OperatorInstance { arguments, .. },
                   ..
               },
               _| {
        let py_src = &arguments[0];
        let py_func_name = &arguments[1];

        let py_func_ident = wc.make_ident("py_func");

        let err_lit = Literal::string(&*format!(
            "Hydroflow 'python' feature must be enabled to use `{}`",
            op_name
        ));

        let write_prologue = quote_spanned! {op_span=>
            #[cfg(feature = "python")]
            let #py_func_ident = {
                ::pyo3::prepare_freethreaded_python();
                let func = ::pyo3::Python::with_gil::<_, ::pyo3::PyResult<::pyo3::Py<::pyo3::PyAny>>>(|py| {
                    Ok(::pyo3::types::PyModule::from_code(
                        py,
                        #py_src,
                        "_filename",
                        "_modulename",
                    )?
                    .getattr(#py_func_name)?
                    .into())
                }).expect("Failed to compile python.");
                #hydroflow.add_state(func)
            };
            #[cfg(not(feature = "python"))]
            ::std::compiler_error!(#err_lit);
        };
        let closure = quote_spanned! {op_span=>
            |x| {
                #[cfg(feature = "python")]
                {
                    // TODO(mingwei): maybe this can be outside the closure?
                    let py_func = #context.state_ref(#py_func_ident);
                    //::pyo3::Python::with_gil(|py| py_func.call1(py, (x,)))
                    ::pyo3::Python::with_gil(|py| py_func.call1(py, x))
                }
                #[cfg(not(feature = "python"))]
                panic!()
            }
        };
        let write_iterator = if is_pull {
            let input = &inputs[0];
            quote_spanned! {op_span=>
                let #ident = #input.map(#closure);
            }
        } else {
            let output = &outputs[0];
            quote_spanned! {op_span=>
                let #ident = #root::pusherator::map::Map::new(#closure, #output);
            }
        };
        Ok(OperatorWriteOutput {
            write_prologue,
            write_iterator,
            ..Default::default()
        })
    },
};