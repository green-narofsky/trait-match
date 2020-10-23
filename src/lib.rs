//! Facilities for matching on the types of expressions.

pub use ::trait_match_proc_macro::sealed;

/// Matching over types on trait objects we can downcast.
#[doc(hidden)]
#[macro_export]
macro_rules! any_match {
    (@[$method:ident]
     @[$success:ident]
     $val:expr => {
        $($p:pat $(in $t:ty)? => $e:expr),* $(,)?
            ; => $base:expr
    }) => {
        {
            let val = $val;
            $(
                if let $success($p) = val.$method $(::<$t>)? () {
                    $e
                }
            )else*
            else {
                $base
            }
        }
    }
}

/// Matching over `dyn Any` trait objects.
#[macro_export]
macro_rules! amatch {
    (move $($t:tt)*) => {
        $crate::any_match! { @[downcast] @[Ok] $($t)* }
    };
    (mut ref $($t:tt)*) => {
        $crate::any_match! { @[downcast_mut] @[Some] $($t)* }
    };
    (ref mut $($t:tt)*) => {
        $crate::amatch! { mut ref $($t)* }
    };
    (mut $($t:tt)*) => {
        $crate::amatch! { mut ref $($t)* }
    };
    (ref $($t:tt)*) => {
        $crate::any_match! { @[downcast_ref] @[Some] $($t)* }
    };
    ($($t:tt)*) => {
        // Default modifier.
        $crate::amatch! { ref $($t)* }
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        use ::core::convert::TryInto;
        let a: Box<dyn ::core::any::Any> = Box::new(10);
        let result = amatch! {
            a => {
                x in i32 => x * 2,
                y in i64 => (y * 10).try_into().unwrap(),
                Some(_) in Option<String> => 11,
                ; => 10
            }
        };
        println!("Result: {:?}", result);
    }
    #[test]
    fn sealed_works() {
        use ::type_match_proc_macro::sealed;
        
    }
}
