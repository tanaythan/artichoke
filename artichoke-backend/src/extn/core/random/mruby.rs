use artichoke_core::eval::Eval;

use crate::convert::RustBackedValue;
use crate::def::{rust_data_free, ClassLike, Define};
use crate::extn::core::exception;
use crate::extn::core::random;
use crate::sys;
use crate::value::Value;
use crate::{Artichoke, ArtichokeError};

pub fn init(interp: &Artichoke) -> Result<(), ArtichokeError> {
    if interp.0.borrow().class_spec::<random::Random>().is_some() {
        return Ok(());
    }

    let spec = interp.0.borrow_mut().def_class::<random::Random>(
        "Random",
        None,
        Some(rust_data_free::<random::Random>),
    );
    spec.borrow_mut().add_self_method(
        "new_seed",
        artichoke_random_self_new_seed,
        sys::mrb_args_req(1),
    );
    spec.borrow_mut()
        .add_self_method("srand", artichoke_random_self_srand, sys::mrb_args_opt(1));
    spec.borrow_mut().add_self_method(
        "urandom",
        artichoke_random_self_urandom,
        sys::mrb_args_req(1),
    );
    spec.borrow_mut().add_method(
        "initialize",
        artichoke_random_initialize,
        sys::mrb_args_opt(1),
    );

    spec.borrow_mut()
        .add_method("==", artichoke_random_eq, sys::mrb_args_opt(1));
    spec.borrow_mut()
        .add_method("bytes", artichoke_random_bytes, sys::mrb_args_req(1));
    spec.borrow_mut()
        .add_method("rand", artichoke_random_rand, sys::mrb_args_opt(1));
    spec.borrow_mut()
        .add_method("seed", artichoke_random_seed, sys::mrb_args_none());
    spec.borrow_mut().mrb_value_is_rust_backed(true);
    spec.borrow().define(interp)?;

    let default = random::default();
    let default = unsafe { default.try_into_ruby(interp, None) }?;
    let mrb = interp.0.borrow().mrb;
    let rclass = spec.borrow().rclass(interp).ok_or(ArtichokeError::New)?;
    unsafe {
        sys::mrb_define_const(
            mrb,
            rclass,
            b"DEFAULT\0".as_ptr() as *const i8,
            default.inner(),
        );
    }

    interp.eval(&include_bytes!("random.rb")[..])?;
    trace!("Patched ENV onto interpreter");
    Ok(())
}

#[no_mangle]
unsafe extern "C" fn artichoke_random_initialize(
    mrb: *mut sys::mrb_state,
    slf: sys::mrb_value,
) -> sys::mrb_value {
    let seed = mrb_get_args!(mrb, optional = 1);
    let interp = unwrap_interpreter!(mrb);
    let result = random::initialize(
        &interp,
        seed.map(|seed| Value::new(&interp, seed)),
        Some(slf),
    );
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(interp, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_random_eq(
    mrb: *mut sys::mrb_state,
    slf: sys::mrb_value,
) -> sys::mrb_value {
    let other = mrb_get_args!(mrb, required = 1);
    let interp = unwrap_interpreter!(mrb);
    let rand = Value::new(&interp, slf);
    let other = Value::new(&interp, other);
    let result = random::eql(&interp, rand, other);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(interp, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_random_bytes(
    mrb: *mut sys::mrb_state,
    slf: sys::mrb_value,
) -> sys::mrb_value {
    let size = mrb_get_args!(mrb, required = 1);
    let interp = unwrap_interpreter!(mrb);
    let rand = Value::new(&interp, slf);
    let size = Value::new(&interp, size);
    let result = random::bytes(&interp, rand, size);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(interp, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_random_rand(
    mrb: *mut sys::mrb_state,
    slf: sys::mrb_value,
) -> sys::mrb_value {
    let max = mrb_get_args!(mrb, optional = 1);
    let interp = unwrap_interpreter!(mrb);
    let rand = Value::new(&interp, slf);
    let max = max.map(|max| Value::new(&interp, max));
    let result = random::rand(&interp, rand, max);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(interp, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_random_seed(
    mrb: *mut sys::mrb_state,
    slf: sys::mrb_value,
) -> sys::mrb_value {
    mrb_get_args!(mrb, none);
    let interp = unwrap_interpreter!(mrb);
    let rand = Value::new(&interp, slf);
    let result = random::seed(&interp, rand);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(interp, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_random_self_new_seed(
    mrb: *mut sys::mrb_state,
    _slf: sys::mrb_value,
) -> sys::mrb_value {
    mrb_get_args!(mrb, none);
    let interp = unwrap_interpreter!(mrb);
    let result = random::new_seed(&interp);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(interp, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_random_self_srand(
    mrb: *mut sys::mrb_state,
    _slf: sys::mrb_value,
) -> sys::mrb_value {
    let number = mrb_get_args!(mrb, optional = 1);
    let interp = unwrap_interpreter!(mrb);
    let number = number.map(|number| Value::new(&interp, number));
    let result = random::srand(&interp, number);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(interp, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_random_self_urandom(
    mrb: *mut sys::mrb_state,
    _slf: sys::mrb_value,
) -> sys::mrb_value {
    let size = mrb_get_args!(mrb, required = 1);
    let interp = unwrap_interpreter!(mrb);
    let size = Value::new(&interp, size);
    let result = random::urandom(&interp, size);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(interp, exception),
    }
}
