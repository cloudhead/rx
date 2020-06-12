use rgx::math::Point2;
use rgx::rect::Rect;

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

pub fn stitch_frames<T: Clone>(mut frames: Vec<Vec<T>>, fw: usize, fh: usize, val: T) -> Vec<T> {
    let nframes = frames.len();
    let width = fw * nframes;

    if nframes == 0 {
        return Vec::with_capacity(0);
    } else if nframes == 1 {
        return frames.remove(0);
    }

    let mut buffer: Vec<T> = vec![val; fw * fh * nframes];

    for (i, frame) in frames.iter().enumerate() {
        for y in 0..fh {
            let offset = i * fw + y * width;
            buffer.splice(
                offset..offset + fw,
                frame[fw * y..fw * y + fw].iter().cloned(),
            );
        }
    }
    buffer
}

pub fn align_u8<T>(data: &[T]) -> &[u8] {
    let (head, body, tail) = unsafe { data.align_to::<u8>() };

    assert!(head.is_empty());
    assert!(tail.is_empty());

    body
}

#[macro_export]
macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key.to_owned(), $val); )*
         map
    }}
}
