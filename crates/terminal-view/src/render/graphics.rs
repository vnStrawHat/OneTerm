//! Bounded store of Sixel images as GPU-ready `RenderImage`s (IN-0028).
//!
//! The engine hands each decoded image out once through the snapshot; the
//! store converts it to GPUI's BGRA layout, keeps it by id, and evicts the
//! oldest entries past 64 images or 64 MB of pixels, releasing their atlas
//! tiles. A cell whose image was evicted paints nothing.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use gpui::{RenderImage, Window};
use oneterm_terminal::GraphicData;

pub(crate) const MAX_IMAGES: usize = 64;
pub(crate) const MAX_BYTES: usize = 64 << 20;

pub(crate) struct StoredImage {
    pub image: Arc<RenderImage>,
    pub width: u32,
    pub height: u32,
}

pub(crate) struct GraphicStore {
    images: HashMap<u64, StoredImage>,
    /// Insertion order with the pixel byte count, for eviction.
    order: VecDeque<(u64, usize)>,
    bytes: usize,
    max_images: usize,
    max_bytes: usize,
}

impl Default for GraphicStore {
    fn default() -> Self {
        Self::with_limits(MAX_IMAGES, MAX_BYTES)
    }
}

impl GraphicStore {
    pub(crate) fn with_limits(max_images: usize, max_bytes: usize) -> Self {
        Self {
            images: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            max_images,
            max_bytes,
        }
    }

    /// Register every new image of a snapshot; returns how many were uploaded.
    pub(crate) fn ingest(&mut self, images: &[Arc<GraphicData>], window: &mut Window) -> u32 {
        let mut uploaded = 0;
        for data in images {
            if self.images.contains_key(&data.id.0) {
                continue;
            }
            let Some(image) = render_image(data) else {
                continue;
            };
            let bytes = data.rgba.len();
            self.images.insert(
                data.id.0,
                StoredImage {
                    image,
                    width: data.width,
                    height: data.height,
                },
            );
            self.order.push_back((data.id.0, bytes));
            self.bytes += bytes;
            uploaded += 1;
            self.evict(window);
        }
        uploaded
    }

    pub(crate) fn get(&self, id: u64) -> Option<&StoredImage> {
        self.images.get(&id)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.images.len()
    }

    fn evict(&mut self, window: &mut Window) {
        while self.order.len() > self.max_images || self.bytes > self.max_bytes {
            let Some((id, bytes)) = self.order.pop_front() else {
                break;
            };
            self.bytes -= bytes;
            if let Some(stored) = self.images.remove(&id)
                && let Err(error) = window.drop_image(stored.image)
            {
                log::debug!("dropping Sixel image {id}: {error}");
            }
        }
    }
}

/// RGBA straight alpha → the BGRA layout `RenderImage` expects (as GPUI's own
/// `img` element does after decoding).
fn render_image(data: &GraphicData) -> Option<Arc<RenderImage>> {
    let mut bgra = data.rgba.clone();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(data.width, data.height, bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneterm_terminal::GraphicId;

    fn data(id: u64, side: u32) -> Arc<GraphicData> {
        Arc::new(GraphicData {
            id: GraphicId(id),
            width: side,
            height: side,
            rgba: vec![200; (side * side * 4) as usize],
        })
    }

    #[gpui::test]
    fn store_uploads_once_and_evicts_oldest(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let mut store = GraphicStore::with_limits(2, usize::MAX);
            assert_eq!(
                store.ingest(&[data(1, 2), data(1, 2)], window),
                1,
                "same id once"
            );
            assert_eq!(store.ingest(&[data(2, 2), data(3, 2)], window), 2);
            assert_eq!(store.len(), 2);
            assert!(store.get(1).is_none(), "oldest evicted by count");
            assert_eq!(store.get(3).map(|s| (s.width, s.height)), Some((2, 2)));

            let mut store = GraphicStore::with_limits(usize::MAX, 4 * 4 * 4 + 1);
            store.ingest(&[data(1, 4), data(2, 2)], window);
            assert!(store.get(1).is_none(), "oldest evicted by bytes");
            assert!(store.get(2).is_some());
        });
    }
}
