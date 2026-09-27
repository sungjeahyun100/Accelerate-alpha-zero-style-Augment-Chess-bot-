//! Bounded, direct Python/JSON-value conversion. No Python serialization calls.
use numpy::{PyArrayDyn, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::{
    IntoPyObjectExt,
    exceptions::{PyTypeError, PyValueError},
    prelude::*,
    types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple},
};
use serde_json::{Map, Number, Value};

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_NODES: usize = 100_000;
const MAX_DEPTH: usize = 64;
const SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Default)]
struct Budget {
    nodes: usize,
    bytes: usize,
}
impl Budget {
    fn charge(&mut self, depth: usize, bytes: usize) -> PyResult<()> {
        self.nodes += 1;
        self.bytes = self.bytes.saturating_add(bytes);
        if depth > MAX_DEPTH || self.nodes > MAX_NODES || self.bytes > MAX_BYTES {
            return Err(PyValueError::new_err(
                "data exceeds nesting (64), node (100000), or byte (8 MiB) limit",
            ));
        }
        Ok(())
    }
}

fn number(value: f64) -> PyResult<Value> {
    if !value.is_finite() {
        return Err(PyValueError::new_err("numbers must be finite"));
    }
    if value.fract() == 0. && value.abs() > SAFE_INTEGER as f64 {
        return Err(PyValueError::new_err(
            "integral numbers must fit the exact IEEE-754 range",
        ));
    }
    Ok(Value::Number(
        Number::from_f64(value).ok_or_else(|| PyValueError::new_err("invalid number"))?,
    ))
}
fn integer(value: i64) -> PyResult<Value> {
    if !(-SAFE_INTEGER..=SAFE_INTEGER).contains(&value) {
        return Err(PyValueError::new_err(
            "integers must fit the exact IEEE-754 range",
        ));
    }
    Ok(Value::Number(value.into()))
}
fn unsigned(value: u64) -> PyResult<Value> {
    if value > SAFE_INTEGER as u64 {
        return Err(PyValueError::new_err(
            "integers must fit the exact IEEE-754 range",
        ));
    }
    Ok(Value::Number(value.into()))
}

pub fn from_python(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    convert(value, 0, &mut Budget::default())
}

fn nested_array(
    values: &mut impl Iterator<Item = Value>,
    shape: &[usize],
    depth: usize,
    budget: &mut Budget,
) -> PyResult<Value> {
    budget.charge(depth, 8)?;
    if shape.is_empty() {
        return values
            .next()
            .ok_or_else(|| PyValueError::new_err("inconsistent array shape"));
    }
    let mut children = Vec::with_capacity(shape[0]);
    for _ in 0..shape[0] {
        children.push(nested_array(values, &shape[1..], depth + 1, budget)?);
    }
    Ok(Value::Array(children))
}

fn convert(value: &Bound<'_, PyAny>, depth: usize, budget: &mut Budget) -> PyResult<Value> {
    budget.charge(depth, 8)?;
    if value.is_none() {
        return Ok(Value::Null);
    }
    if value.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(value.extract()?));
    }
    if value.is_instance_of::<PyInt>() {
        return integer(
            value
                .extract::<i64>()
                .map_err(|_| PyValueError::new_err("integer outside supported exact range"))?,
        );
    }
    if value.is_instance_of::<PyFloat>() {
        return number(value.extract()?);
    }
    if let Ok(text) = value.cast::<PyString>() {
        let text = text.to_str()?;
        budget.charge(depth, text.len())?;
        return Ok(Value::String(text.into()));
    }
    if let Ok(dict) = value.cast::<PyDict>() {
        if dict.len() > MAX_NODES {
            return Err(PyValueError::new_err("object exceeds node limit"));
        }
        let mut object = Map::new();
        for (key, child) in dict.iter() {
            let key = key
                .cast::<PyString>()
                .map_err(|_| PyTypeError::new_err("object keys must be strings"))?
                .to_str()?
                .to_owned();
            budget.charge(depth + 1, key.len())?;
            object.insert(key, convert(&child, depth + 1, budget)?);
        }
        return Ok(Value::Object(object));
    }
    if let Ok(list) = value.cast::<PyList>() {
        if list.len() > MAX_NODES {
            return Err(PyValueError::new_err("array exceeds node limit"));
        }
        return list
            .iter()
            .map(|child| convert(&child, depth + 1, budget))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array);
    }
    if let Ok(tuple) = value.cast::<PyTuple>() {
        if tuple.len() > MAX_NODES {
            return Err(PyValueError::new_err("array exceeds node limit"));
        }
        return tuple
            .iter()
            .map(|child| convert(&child, depth + 1, budget))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array);
    }
    // A readonly NumPy borrow remains attached to Python. We copy it into owned
    // values before any detach; no borrowed array or Python reference crosses
    // into the engine. Strided arrays are read in logical element order.
    macro_rules! array {
        ($element:ty, $convert:expr) => {
            if let Ok(array) = value.cast::<PyArrayDyn<$element>>() {
                if array.len() > MAX_NODES
                    || array.shape().iter().any(|dim| *dim > MAX_NODES)
                    || array.ndim() > MAX_DEPTH - depth
                {
                    return Err(PyValueError::new_err(
                        "NumPy array exceeds conversion limits",
                    ));
                }
                let shape = array.shape().to_vec();
                let owned = array
                    .try_readonly()?
                    .as_array()
                    .iter()
                    .copied()
                    .map($convert)
                    .collect::<PyResult<Vec<Value>>>()?;
                return nested_array(&mut owned.into_iter(), &shape, depth + 1, budget);
            }
        };
    }
    array!(f64, number);
    array!(f32, |v| number(f64::from(v)));
    array!(i64, integer);
    array!(i32, |v| integer(i64::from(v)));
    array!(u64, unsigned);
    array!(u32, |v| unsigned(u64::from(v)));
    array!(u8, |v| unsigned(u64::from(v)));
    array!(bool, |v| Ok(Value::Bool(v)));
    Err(PyTypeError::new_err(
        "expected JSON values or a supported numerical NumPy array",
    ))
}

pub fn validate(value: &Value) -> PyResult<()> {
    fn visit(value: &Value, depth: usize, budget: &mut Budget) -> PyResult<()> {
        budget.charge(depth, 8)?;
        match value {
            Value::Number(v) => {
                if let Some(v) = v.as_i64() {
                    integer(v)?;
                } else if let Some(v) = v.as_u64() {
                    unsigned(v)?;
                } else {
                    number(
                        v.as_f64()
                            .ok_or_else(|| PyValueError::new_err("invalid number"))?,
                    )?;
                }
            }
            Value::String(v) => budget.charge(depth, v.len())?,
            Value::Array(v) => {
                for child in v {
                    visit(child, depth + 1, budget)?;
                }
            }
            Value::Object(v) => {
                for (key, child) in v {
                    budget.charge(depth + 1, key.len())?;
                    visit(child, depth + 1, budget)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(value, 0, &mut Budget::default())
}

pub fn to_python(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(v) => v.into_py_any(py),
        Value::String(v) => v.into_py_any(py),
        Value::Number(v) => {
            if let Some(v) = v.as_i64() {
                v.into_py_any(py)
            } else if let Some(v) = v.as_u64() {
                v.into_py_any(py)
            } else {
                v.as_f64()
                    .ok_or_else(|| PyValueError::new_err("invalid number"))?
                    .into_py_any(py)
            }
        }
        Value::Array(v) => Ok(PyList::new(
            py,
            v.iter()
                .map(|v| to_python(py, v))
                .collect::<PyResult<Vec<_>>>()?,
        )?
        .into_any()
        .unbind()),
        Value::Object(v) => {
            let dict = PyDict::new(py);
            for (key, child) in v {
                dict.set_item(key, to_python(py, child)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}
