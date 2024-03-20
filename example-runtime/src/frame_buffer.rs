use std::mem;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use wasmtime::component::Resource;

use crate::graphics_context::{GraphicsContext, GraphicsContextBuffer};
use crate::{HostState};

#[derive(Clone)]
pub struct Surface {
    pub(super) surface: Arc<Mutex<softbuffer::Surface>>,
}
unsafe impl Send for Surface {}
unsafe impl Sync for Surface {}
impl From<softbuffer::Surface> for Surface {
    fn from(surface: softbuffer::Surface) -> Self {
        Surface {
            surface: Arc::new(Mutex::new(surface)),
        }
    }
}
impl Surface {
    pub fn buffer_mut<'a>(&'a mut self) -> FrameBuffer {
        let mut surface = self.surface.lock().unwrap();
        let buff = surface.buffer_mut().unwrap();
        // TODO: use ouroboros?
        let buff: softbuffer::Buffer<'static> = unsafe { mem::transmute(buff) };
        buff.into()
    }

    pub fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) {
        self.surface.lock().unwrap().resize(width, height).unwrap();
    }
}

pub struct FrameBuffer {
    // Never none
    buffer: Arc<Mutex<Option<softbuffer::Buffer<'static>>>>,
}
unsafe impl Send for FrameBuffer {}
unsafe impl Sync for FrameBuffer {}
impl From<softbuffer::Buffer<'static>> for FrameBuffer {
    fn from(buffer: softbuffer::Buffer<'static>) -> Self {
        FrameBuffer {
            buffer: Arc::new(Mutex::new(Some(buffer))),
        }
    }
}

impl crate::component::webgpu::frame_buffer::Host for HostState {
    fn connect_graphics_context(
        &mut self,
        _: Resource<GraphicsContext>,
    ) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl crate::component::webgpu::frame_buffer::HostFrameBuffer for HostState {
    fn from_graphics_buffer(
        &mut self,
        buffer: Resource<crate::graphics_context::GraphicsContextBuffer>,
    ) -> wasmtime::Result<Resource<FrameBuffer>> {
        let host_buffer: GraphicsContextBuffer = self.table.delete(buffer).unwrap();
        if let GraphicsContextBuffer::FrameBuffer(host_buffer) = host_buffer {
            Ok(self.table.push(host_buffer).unwrap())
        } else {
            panic!("Context not connected to webgpu");
        }
    }

    fn length(&mut self, buffer: Resource<FrameBuffer>) -> wasmtime::Result<u32> {
        let buffer = self.table.get(&buffer).unwrap();
        let len = buffer.buffer.lock().unwrap().as_ref().unwrap().len();
        Ok(len as u32)
    }

    fn get(&mut self, buffer: Resource<FrameBuffer>, i: u32) -> wasmtime::Result<u32> {
        let buffer = self.table.get(&buffer).unwrap();
        let val = *buffer
            .buffer
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .get(i as usize)
            .unwrap();
        Ok(val)
    }

    fn set(&mut self, buffer: Resource<FrameBuffer>, i: u32, val: u32) -> wasmtime::Result<()> {
        let buffer = self.table.get_mut(&buffer).unwrap();
        buffer.buffer.lock().unwrap().as_mut().unwrap()[i as usize] = val as u32;
        Ok(())
    }

    fn drop(&mut self, _rep: Resource<FrameBuffer>) -> wasmtime::Result<()> {
        // todo!()
        Ok(())
    }
}
