//! # Ruby Exception Hierarchy
//!
//! The built-in subclasses of
//! [`Exception`](https://ruby-doc.org/core-2.6.3/Exception.html) are:
//!
//! - `NoMemoryError`
//! - `ScriptError`
//!   - `LoadError`
//!   - `NotImplementedError`
//!   - `SyntaxError`
//! - `SecurityError`
//! - `SignalException`
//!   - `Interrupt`
//! - `StandardError` -- default for `rescue`
//!   - `ArgumentError`
//!     - `UncaughtThrowError`
//!   - `EncodingError`
//!   - `FiberError`
//!   - `IOError`
//!     - `EOFError`
//!   - `IndexError`
//!     - `KeyError`
//!     - `StopIteration`
//!   - `LocalJumpError`
//!   - `NameError`
//!     - `NoMethodError`
//!   - `RangeError`
//!     - `FloatDomainError`
//!   - `RegexpError`
//!   - `RuntimeError` -- default for `raise`
//!     - `FrozenError`
//!   - `SystemCallError`
//!     - `Errno::*`
//!   - `ThreadError`
//!   - `TypeError`
//!   - `ZeroDivisionError`
//! - `SystemExit`
//! - `SystemStackError`
//! - `fatal` -- impossible to rescue

use artichoke_core::eval::Eval;
#[cfg(feature = "artichoke-debug")]
use backtrace::Backtrace;
use std::borrow::Cow;
use std::cell::RefCell;
use std::error;
use std::fmt;
use std::rc::Rc;

use crate::class;
use crate::convert::Convert;
use crate::def::{ClassLike, Define};
use crate::sys;
use crate::{Artichoke, ArtichokeError};

pub fn init(interp: &Artichoke) -> Result<(), ArtichokeError> {
    let exception = Exception::init(interp, None)?;
    NoMemoryError::init(interp, Some(Rc::clone(&exception)))?;
    let script = ScriptError::init(interp, Some(Rc::clone(&exception)))?;
    LoadError::init(interp, Some(Rc::clone(&script)))?;
    NotImplementedError::init(interp, Some(Rc::clone(&script)))?;
    SyntaxError::init(interp, Some(Rc::clone(&script)))?;
    SecurityError::init(interp, Some(Rc::clone(&exception)))?;
    let signal = SignalException::init(interp, Some(Rc::clone(&exception)))?;
    Interrupt::init(interp, Some(Rc::clone(&signal)))?;
    // Default for `rescue`.
    let standard = StandardError::init(interp, Some(Rc::clone(&exception)))?;
    let argument = ArgumentError::init(interp, Some(Rc::clone(&standard)))?;
    UncaughtThrowError::init(interp, Some(Rc::clone(&argument)))?;
    EncodingError::init(interp, Some(Rc::clone(&standard)))?;
    FiberError::init(interp, Some(Rc::clone(&standard)))?;
    let io = IOError::init(interp, Some(Rc::clone(&standard)))?;
    EOFError::init(interp, Some(Rc::clone(&io)))?;
    let index = IndexError::init(interp, Some(Rc::clone(&standard)))?;
    KeyError::init(interp, Some(Rc::clone(&index)))?;
    StopIteration::init(interp, Some(Rc::clone(&index)))?;
    LocalJumpError::init(interp, Some(Rc::clone(&standard)))?;
    let name = NameError::init(interp, Some(Rc::clone(&standard)))?;
    NoMethodError::init(interp, Some(Rc::clone(&name)))?;
    let range = RangeError::init(interp, Some(Rc::clone(&standard)))?;
    FloatDomainError::init(interp, Some(Rc::clone(&range)))?;
    RegexpError::init(interp, Some(Rc::clone(&standard)))?;
    // Default `Exception` type for `raise`.
    let runtime = RuntimeError::init(interp, Some(Rc::clone(&standard)))?;
    FrozenError::init(interp, Some(Rc::clone(&runtime)))?;
    let _syscall = SystemCallError::init(interp, Some(Rc::clone(&standard)))?;
    ThreadError::init(interp, Some(Rc::clone(&standard)))?;
    TypeError::init(interp, Some(Rc::clone(&standard)))?;
    ZeroDivisionError::init(interp, Some(Rc::clone(&standard)))?;
    SystemExit::init(interp, Some(Rc::clone(&exception)))?;
    SystemStackError::init(interp, Some(Rc::clone(&exception)))?;
    Fatal::init(interp, Some(Rc::clone(&exception)))?;

    interp.eval(&include_bytes!("exception.rb")[..])?;

    Ok(())
}

/// Raise implementation for `RubyException` boxed trait objects.
///
/// # Safety
///
/// This function unwinds the stack with `longjmp`, which will ignore all Rust
/// landing pads for panics and exit routines for cleaning up borrows. Callers
/// should ensure that only [`Copy`] items are alive in the current stack frame.
///
/// Because this precondition must hold for all frames between the caller and
/// the closest [`sys::mrb_protect`] landing pad, this function should only be
/// called in the entrypoint into Rust from mruby.
pub unsafe fn raise(interp: Artichoke, exception: impl RubyException) -> ! {
    // Ensure the borrow is out of scope by the time we eval code since
    // Rust-backed files and types may need to mutably borrow the `Artichoke` to
    // get access to the underlying `ArtichokeState`.
    let mrb = interp.0.borrow().mrb;

    let spec = exception.class();
    let borrow = spec.borrow();
    let eclass = if let Some(rclass) = borrow.rclass(&interp) {
        rclass
    } else {
        error!("unable to raise {}", borrow.name());
        panic!("unable to raise {}", borrow.name());
    };
    let formatargs = interp.convert(exception.message()).inner();
    // `mrb_sys_raise` will call longjmp which will unwind the stack.
    // Any non-`Copy` objects that we haven't cleaned up at this point will
    // leak, so drop everything.
    drop(borrow);
    drop(spec);
    drop(interp);
    drop(exception);

    sys::mrb_raisef(mrb, eclass, b"%S\0".as_ptr() as *const i8, formatargs);
    unreachable!("mrb_raisef will unwind the stack with longjmp");
}

#[allow(clippy::module_name_repetitions)]
pub trait RubyException
where
    Self: 'static,
{
    fn message(&self) -> &[u8];

    fn class(&self) -> Rc<RefCell<class::Spec>>;
}

macro_rules! ruby_exception_impl {
    ($exception:ident) => {
        pub struct $exception {
            interp: Artichoke,
            message: Cow<'static, [u8]>,
            #[cfg(feature = "artichoke-debug")]
            backtrace: Backtrace,
        }

        impl $exception {
            fn init(
                interp: &Artichoke,
                superclass: Option<Rc<RefCell<class::Spec>>>,
            ) -> Result<Rc<RefCell<class::Spec>>, ArtichokeError> {
                let class =
                    interp
                        .0
                        .borrow_mut()
                        .def_class::<Self>(stringify!($exception), None, None);
                if let Some(superclass) = superclass {
                    class.borrow_mut().with_super_class(superclass);
                }
                class.borrow().define(interp)?;
                Ok(class)
            }

            pub fn new<S>(interp: &Artichoke, message: S) -> Self
            where
                S: Into<Cow<'static, str>>,
            {
                let message = match message.into() {
                    Cow::Borrowed(s) => Cow::Borrowed(s.as_bytes()),
                    Cow::Owned(s) => Cow::Owned(s.into_bytes()),
                };
                Self {
                    interp: interp.clone(),
                    message,
                    #[cfg(feature = "artichoke-debug")]
                    backtrace: Backtrace::new(),
                }
            }

            pub fn new_raw<S>(interp: &Artichoke, message: S) -> Self
            where
                S: Into<Cow<'static, [u8]>>,
            {
                Self {
                    interp: interp.clone(),
                    message: message.into(),
                    #[cfg(feature = "artichoke-debug")]
                    backtrace: Backtrace::new(),
                }
            }
        }

        #[allow(clippy::use_self)]
        impl From<$exception> for Box<dyn RubyException>
        where
            $exception: RubyException,
        {
            fn from(exception: $exception) -> Box<dyn RubyException> {
                Box::new(exception)
            }
        }

        #[allow(clippy::use_self)]
        impl From<Box<$exception>> for Box<dyn RubyException>
        where
            $exception: RubyException,
        {
            fn from(exception: Box<$exception>) -> Box<dyn RubyException> {
                exception
            }
        }

        impl RubyException for $exception {
            fn message(&self) -> &[u8] {
                self.message.as_ref()
            }

            fn class(&self) -> Rc<RefCell<class::Spec>> {
                if let Some(spec) = self.interp.0.borrow().class_spec::<Self>() {
                    spec
                } else {
                    panic!("Unknown Exception class spec");
                }
            }
        }

        impl fmt::Debug for $exception
        where
            $exception: RubyException,
        {
            #[cfg(feature = "artichoke-debug")]
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let class = self.class();
                let borrow = class.borrow();
                let classname = borrow.name();
                let message = String::from_utf8_lossy(self.message());
                write!(f, "{} ({})", classname, message)?;
                write!(f, "\n{:?}", self.backtrace)
            }

            #[cfg(not(feature = "artichoke-debug"))]
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let class = self.class();
                let borrow = class.borrow();
                let classname = borrow.name();
                let message = String::from_utf8_lossy(self.message());
                write!(f, "{} ({})", classname, message)
            }
        }

        impl fmt::Display for $exception
        where
            $exception: RubyException,
        {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let class = self.class();
                let borrow = class.borrow();
                let classname = borrow.name();
                let message = String::from_utf8_lossy(self.message());
                write!(f, "{} ({})", classname, message)
            }
        }

        impl error::Error for $exception {
            fn description(&self) -> &str {
                concat!("Ruby Exception: ", stringify!($exception))
            }

            fn cause(&self) -> Option<&dyn error::Error> {
                None
            }
        }
    };
}

impl RubyException for Box<dyn RubyException> {
    fn message(&self) -> &[u8] {
        self.as_ref().message()
    }

    fn class(&self) -> Rc<RefCell<class::Spec>> {
        self.as_ref().class()
    }
}

impl fmt::Debug for Box<dyn RubyException> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let class = self.class();
        let borrow = class.borrow();
        let classname = borrow.name();
        let message = String::from_utf8_lossy(self.message());
        write!(f, "{} ({})", classname, message)
    }
}

impl fmt::Display for Box<dyn RubyException> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let class = self.class();
        let borrow = class.borrow();
        let classname = borrow.name();
        let message = String::from_utf8_lossy(self.message());
        write!(f, "{} ({})", classname, message)
    }
}

impl error::Error for Box<dyn RubyException> {
    fn description(&self) -> &str {
        "Ruby Exception: "
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}

ruby_exception_impl!(Exception);
ruby_exception_impl!(NoMemoryError);
ruby_exception_impl!(ScriptError);
ruby_exception_impl!(LoadError);
ruby_exception_impl!(NotImplementedError);
ruby_exception_impl!(SyntaxError);
ruby_exception_impl!(SecurityError);
ruby_exception_impl!(SignalException);
ruby_exception_impl!(Interrupt);
// Default for `rescue`.
ruby_exception_impl!(StandardError);
ruby_exception_impl!(ArgumentError);
ruby_exception_impl!(UncaughtThrowError);
ruby_exception_impl!(EncodingError);
ruby_exception_impl!(FiberError);
ruby_exception_impl!(IOError);
ruby_exception_impl!(EOFError);
ruby_exception_impl!(IndexError);
ruby_exception_impl!(KeyError);
ruby_exception_impl!(StopIteration);
ruby_exception_impl!(LocalJumpError);
ruby_exception_impl!(NameError);
ruby_exception_impl!(NoMethodError);
ruby_exception_impl!(RangeError);
ruby_exception_impl!(FloatDomainError);
ruby_exception_impl!(RegexpError);
// Default `Exception` type for `raise`.
ruby_exception_impl!(RuntimeError);
ruby_exception_impl!(FrozenError);
ruby_exception_impl!(SystemCallError);
// ruby_exception_impl!(Errno::*);
ruby_exception_impl!(ThreadError);
ruby_exception_impl!(TypeError);
ruby_exception_impl!(ZeroDivisionError);
ruby_exception_impl!(SystemExit);
ruby_exception_impl!(SystemStackError);
// Fatal interpreter error. Impossible to rescue.
ruby_exception_impl!(Fatal);

#[cfg(test)]
mod tests {
    use artichoke_core::eval::Eval;
    use artichoke_core::file::File;

    use crate::def::{ClassLike, Define};
    use crate::exception::Exception;
    use crate::extn::core::exception::RuntimeError;
    use crate::sys;
    use crate::{Artichoke, ArtichokeError};

    struct Run;

    impl Run {
        unsafe extern "C" fn run(mrb: *mut sys::mrb_state, _slf: sys::mrb_value) -> sys::mrb_value {
            let interp = unwrap_interpreter!(mrb);
            let exc = RuntimeError::new(&interp, "something went wrong");
            super::raise(interp, exc)
        }
    }

    impl File for Run {
        type Artichoke = Artichoke;

        fn require(interp: &Artichoke) -> Result<(), ArtichokeError> {
            let spec = interp.0.borrow_mut().def_class::<Self>("Run", None, None);
            spec.borrow_mut()
                .add_self_method("run", Self::run, sys::mrb_args_none());
            spec.borrow().define(&interp)?;
            Ok(())
        }
    }

    #[test]
    fn raise() {
        let interp = crate::interpreter().expect("init");
        Run::require(&interp).unwrap();
        let value = interp.eval(b"Run.run").map(|_| ());
        let expected = Exception::new(
            "RuntimeError",
            "something went wrong",
            Some(vec!["(eval):1".to_owned()]),
            "(eval):1: something went wrong (RuntimeError)",
        );
        assert_eq!(value, Err(ArtichokeError::Exec(expected.to_string())));
    }
}
