use cgmath::Point2;
use rgx::core::Rect;

pub fn clamp(p: &mut Point2<i32>, rect: Rect<i32>) {
    if p.x < rect.x1 {
        p.x = rect.x1;
    }
    if p.y < rect.y1 {
        p.y = rect.y1;
    }
    if p.x > rect.x2 {
        p.x = rect.x2;
    }
    if p.y > rect.y2 {
        p.y = rect.y2;
    }
}

#[macro_export]
macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key.to_string(), $val); )*
         map
    }}
}
