use type_match::sealed;

struct Heya;
struct Boog;

#[sealed(Heya, Boog)]
#[seal(enum = LolEnum, upcast)]
trait Lol {
    fn lol() -> i32;
}

impl Lol for Heya {
    fn lol() -> i32 {
        3
    }
}
impl Lol for Boog {
    fn lol() -> i32 {
        4
    }
}


fn hrm<T: Lol>(v: T) -> &'static str {
    match v.upcast() {
        LolEnum::Heya(_) => "Heya",
        LolEnum::Boog(_) => "Boog",
    }
}

fn main() {
    println!("Result: {}", hrm(Heya));
}
