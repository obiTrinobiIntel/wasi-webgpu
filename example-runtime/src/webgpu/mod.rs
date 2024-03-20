use std::borrow::Cow;
use wasmtime::component::Resource;

use crate::component::webgpu::webgpu;
use crate::graphics_context::{GraphicsContext, GraphicsContextBuffer};
use crate::HostState;

use self::to_core_conversions::ToCore;

// ToCore trait used for resources, records, and variants.
// Into trait used for enums, since they never need table access.
mod enum_conversions;
mod to_core_conversions;

fn map_callback(status: Result<(), wgpu_core::resource::BufferAccessError>) {
    if let Err(e) = status {
        panic!("Buffer map error: {}", e);
    }
}

#[derive(Clone, Copy)]
pub struct Timer {
    pub start : std::time::Instant,
}

#[derive(Clone, Copy)]
pub struct Device {
    pub device: wgpu_core::id::DeviceId,
    // only needed when calling surface.get_capabilities in connect_graphics_context. If table would have a way to get parent from child, we could get it from device.
    pub adapter: wgpu_core::id::AdapterId,
}

#[derive(Clone, Copy)]
pub struct Pipeline {
    pub pipe: wgpu_core::id::ComputePipelineId,

    pub device: wgpu_core::id::DeviceId,
    // only needed when calling surface.get_capabilities in connect_graphics_context. If table would have a way to get parent from child, we could get it from device.
    
}

#[derive(Clone, Copy)]
pub struct Buffer {
    pub buf: wgpu_core::id::BufferId,
    pub dev: wgpu_core::id::DeviceId,
}

impl webgpu::Host for HostState {
    fn get_gpu(&mut self) -> wasmtime::Result<Resource<webgpu::Gpu>> {
        Ok(Resource::new_own(0))
    }
    fn new_timer(&mut self) -> wasmtime::Result<Resource<webgpu::Timer>> {
        let daq = self
        .table
        .push(
            Timer {
                start: std::time::Instant::now(),
            }
        )
        .unwrap();
        Ok(daq)
    }
}


impl webgpu::HostTimer for HostState {
    
    fn tick(&mut self, timer: Resource<webgpu::Timer>)-> wasmtime::Result<()> {
        println!("tick");
        let mut timer_inst = *self.table.get(&timer).unwrap();
        timer_inst.start = std::time::Instant::now();
        Ok(())
    }

    fn tock(&mut self, timer: Resource<webgpu::Timer>)-> wasmtime::Result<()> {
        let timer_inst = self.table.get(&timer).unwrap();
        println!("Elapsed time: {:?}", timer_inst.start.elapsed());
        Ok(())
    }

    fn drop(&mut self, _rep: Resource<webgpu::Timer>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl webgpu::HostGpuDevice for HostState {
    fn connect_graphics_context(
        &mut self,
        _: Resource<Device>,
        _: Resource<GraphicsContext>,
    ) -> wasmtime::Result<()> {
       
        Ok(())
    }

    fn create_command_encoder(
        &mut self,
        device: Resource<Device>,
        descriptor: Option<webgpu::GpuCommandEncoderDescriptor>,
    ) -> wasmtime::Result<Resource<wgpu_core::id::CommandEncoderId>> {
        let host_daq = self.table.get(&device).unwrap();

        let command_encoder = core_result(
            self.instance
                .device_create_command_encoder::<crate::Backend>(
                    host_daq.device,
                    &descriptor
                        .map(|d| d.to_core(&self.table))
                        .unwrap_or_default(),
                    (),
                ),
        )
        .unwrap();

        Ok(self.table.push_child(command_encoder, &device).unwrap())
    }

    fn create_shader_module(
        &mut self,
        device: Resource<Device>,
        descriptor: webgpu::GpuShaderModuleDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuShaderModule>> {
        let device = self.table.get(&device).unwrap();

        let code =
            wgpu_core::pipeline::ShaderModuleSource::Wgsl(Cow::Owned(descriptor.code.to_owned()));
        let shader = core_result(self.instance.device_create_shader_module::<crate::Backend>(
            device.device,
            &descriptor.to_core(&self.table),
            code,
            (),
        ))
        .unwrap();

        Ok(self.table.push(shader).unwrap())
    }

    fn create_render_pipeline(
        &mut self,
        device: Resource<Device>,
        descriptor: webgpu::GpuRenderPipelineDescriptor,
    ) -> wasmtime::Result<Resource<wgpu_core::id::RenderPipelineId>> {
        let host_device = self.table.get(&device).unwrap();

        let descriptor = descriptor.to_core(&self.table);

        let implicit_pipeline_ids = match descriptor.layout {
            Some(_) => None,
            None => Some(wgpu_core::device::ImplicitPipelineIds {
                root_id: (),
                group_ids: &[(); wgpu_core::MAX_BIND_GROUPS],
            }),
        };
        let render_pipeline = core_result(
            self.instance
                .device_create_render_pipeline::<crate::Backend>(
                    host_device.device,
                    &descriptor,
                    (),
                    implicit_pipeline_ids,
                ),
        )
        .unwrap();

        Ok(self.table.push_child(render_pipeline, &device).unwrap())
    }

    fn queue(&mut self, device: Resource<Device>) -> wasmtime::Result<Resource<Device>> {
        Ok(Resource::new_own(device.rep()))
    }

    fn features(
        &mut self,
        device: Resource<webgpu::GpuDevice>,
    ) -> wasmtime::Result<Resource<webgpu::GpuSupportedFeatures>> {
        let device = self.table.get(&device).unwrap();
        let features = self
            .instance
            .device_features::<crate::Backend>(device.device)
            .unwrap();
        Ok(self.table.push(features).unwrap())
    }

    fn limits(
        &mut self,
        _device: Resource<webgpu::GpuDevice>,
    ) -> wasmtime::Result<Resource<webgpu::GpuSupportedLimits>> {
        todo!()
    }

    fn destroy(&mut self, _device: Resource<webgpu::GpuDevice>) -> wasmtime::Result<()> {
        todo!()
    }

    fn create_buffer(
        &mut self,
        device: Resource<webgpu::GpuDevice>,
        descriptor: webgpu::GpuBufferDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuBuffer>> {
        let device = self.table.get(&device).unwrap();
        let buf_context = descriptor.clone().context;

        let desc = descriptor.to_core(&self.table);
        let buffer = core_result(self.instance.device_create_buffer::<crate::Backend>(
            device.device,
            &desc,
            (),
        ))
        .unwrap();
        let empty_vec: Vec<u8> = Vec::new();
        let buffer_data = buf_context.unwrap_or(empty_vec);
        let unpadded_size = buffer_data.len();
       
        if unpadded_size > 0{
            let (data, size) = self.instance.buffer_get_mapped_range::<crate::Backend>(
                buffer,
                0,
                Some(unpadded_size.try_into().unwrap()),
            ).unwrap();
            unsafe {
                let contents:  &mut [u8]= std::slice::from_raw_parts_mut(data, size as usize);
                contents.copy_from_slice(&buffer_data[..unpadded_size]);
            }
            let _ = self.instance.buffer_unmap::<crate::Backend>(buffer);
        }
        let daq = self
            .table
            .push(
                Buffer {
                    buf: buffer,
                    dev: device.device,
                }
            )
            .unwrap();
        println!("Create Buf{:?}", buffer);
       
        Ok(daq)
    }

    fn create_texture(
        &mut self,
        device: Resource<webgpu::GpuDevice>,
        descriptor: webgpu::GpuTextureDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuTexture>> {
        let device = *self.table.get(&device).unwrap();
        let texture = core_result(self.instance.device_create_texture::<crate::Backend>(
            device.device,
            &descriptor.to_core(&self.table),
            (),
        ))
        .unwrap();

        Ok(self.table.push(texture).unwrap())
    }

    fn create_sampler(
        &mut self,
        device: Resource<webgpu::GpuDevice>,
        descriptor: Option<webgpu::GpuSamplerDescriptor>,
    ) -> wasmtime::Result<Resource<webgpu::GpuSampler>> {
        let device = self.table.get(&device).unwrap();

        let descriptor = descriptor.unwrap();

        let sampler = core_result(self.instance.device_create_sampler::<crate::Backend>(
            device.device,
            &descriptor.to_core(&self.table),
            (),
        ))
        .unwrap();

        Ok(self.table.push(sampler).unwrap())
    }

    fn import_external_texture(
        &mut self,
        _device: Resource<webgpu::GpuDevice>,
        _descriptor: webgpu::GpuExternalTextureDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuExternalTexture>> {
        todo!()
    }

    fn create_bind_group_layout(
        &mut self,
        device: Resource<webgpu::GpuDevice>,
        descriptor: webgpu::GpuBindGroupLayoutDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuBindGroupLayout>> {
        let device = self.table.get(&device).unwrap();

        let bind_group_layout = core_result(
            self.instance
                .device_create_bind_group_layout::<crate::Backend>(
                    device.device,
                    &descriptor.to_core(&self.table),
                    (),
                ),
        )
        .unwrap();

        Ok(self.table.push(bind_group_layout).unwrap())
    }

    fn create_pipeline_layout(
        &mut self,
        device: Resource<webgpu::GpuDevice>,
        descriptor: webgpu::GpuPipelineLayoutDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuPipelineLayout>> {
        let device = *self.table.get(&device).unwrap();

        let pipeline_layout = core_result(
            self.instance
                .device_create_pipeline_layout::<crate::Backend>(
                    device.device,
                    &descriptor.to_core(&self.table),
                    (),
                ),
        )
        .unwrap();

        Ok(self.table.push(pipeline_layout).unwrap())
    }

    fn create_bind_group(
        &mut self,
        device: Resource<webgpu::GpuDevice>,
        descriptor: webgpu::GpuBindGroupDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuBindGroup>> {
        let device = *self.table.get(&device).unwrap();

        let bind_group = core_result(self.instance.device_create_bind_group::<crate::Backend>(
            device.device,
            &descriptor.to_core(&self.table),
            (),
        ))
        .unwrap();

        Ok(self.table.push(bind_group).unwrap())
    }

    fn create_compute_pipeline(
        &mut self,
        device: Resource<webgpu::GpuDevice>,
        descriptor: webgpu::GpuComputePipelineDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuComputePipeline>> {
        
        let host_device = self.table.get(&device).unwrap();

        let descriptor = descriptor.to_core(&self.table);

        let implicit_pipeline_ids = match descriptor.layout {
            Some(_) => None,
            None => Some(wgpu_core::device::ImplicitPipelineIds {
                root_id: (),
                group_ids: &[(); wgpu_core::MAX_BIND_GROUPS],
            }),
        };
        let compute_pipeline = core_result(
            self.instance
                .device_create_compute_pipeline::<crate::Backend>(
                    host_device.device,
                    &descriptor,
                    (),
                    implicit_pipeline_ids,
                )
            )
        .unwrap();
        let daq = self
            .table
            .push_child(
                Pipeline {
                    pipe: compute_pipeline,
                    device: host_device.device,
                },
                &device,
            )
            .unwrap();

        Ok(daq)
    }

    // fn create_compute_pipeline_async(
    //     &mut self,
    //     self_: Resource<webgpu::GpuDevice>,
    //     descriptor: webgpu::GpuComputePipelineDescriptor,
    // ) -> wasmtime::Result<Resource<webgpu::GpuComputePipeline>> {
    //     todo!()
    // }

    // fn create_render_pipeline_async(
    //     &mut self,
    //     self_: Resource<webgpu::GpuDevice>,
    //     descriptor: webgpu::GpuRenderPipelineDescriptor,
    // ) -> wasmtime::Result<Resource<wgpu_core::id::RenderPipelineId>> {
    //     todo!()
    // }

    fn create_render_bundle_encoder(
        &mut self,
        _device: Resource<webgpu::GpuDevice>,
        _descriptor: webgpu::GpuRenderBundleEncoderDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuRenderBundleEncoder>> {
        todo!()
    }

    fn create_query_set(
        &mut self,
        _device: Resource<webgpu::GpuDevice>,
        _descriptor: webgpu::GpuQuerySetDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuQuerySet>> {
        todo!()
    }

    fn label(&mut self, _device: Resource<webgpu::GpuDevice>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _device: Resource<webgpu::GpuDevice>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn lost(
        &mut self,
        _device: Resource<webgpu::GpuDevice>,
    ) -> wasmtime::Result<Resource<webgpu::GpuDeviceLostInfo>> {
        todo!()
    }

    fn push_error_scope(
        &mut self,
        _device: Resource<webgpu::GpuDevice>,
        _filter: webgpu::GpuErrorFilter,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn pop_error_scope(
        &mut self,
        _device: Resource<webgpu::GpuDevice>,
    ) -> wasmtime::Result<Resource<webgpu::GpuError>> {
        todo!()
    }

    fn onuncapturederror(
        &mut self,
        _device: Resource<webgpu::GpuDevice>,
    ) -> wasmtime::Result<Resource<webgpu::EventHandler>> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuDevice>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl webgpu::HostGpuTexture for HostState {
    fn from_graphics_buffer(
        &mut self,
        buffer: Resource<GraphicsContextBuffer>,
    ) -> wasmtime::Result<Resource<wgpu_core::id::TextureId>> {
        let host_buffer = self.table.delete(buffer).unwrap();
        if let GraphicsContextBuffer::Webgpu(host_buffer) = host_buffer {
            Ok(self.table.push(host_buffer).unwrap())
        } else {
            panic!("Context not connected to webgpu");
        }
    }

    fn create_view(
        &mut self,
        texture: Resource<wgpu_core::id::TextureId>,
        descriptor: Option<webgpu::GpuTextureViewDescriptor>,
    ) -> wasmtime::Result<Resource<wgpu_core::id::TextureViewId>> {
        let texture_id = *self.table.get(&texture).unwrap();
        let texture_view = core_result(
            self.instance.texture_create_view::<crate::Backend>(
                texture_id,
                &descriptor
                    .map(|d| d.to_core(&self.table))
                    .unwrap_or_default(),
                (),
            ),
        )
        .unwrap();
        Ok(self.table.push(texture_view).unwrap())
    }

    fn drop(&mut self, _rep: Resource<wgpu_core::id::TextureId>) -> wasmtime::Result<()> {
        // TODO:
        Ok(())
    }

    fn destroy(&mut self, _self_: Resource<webgpu::GpuTexture>) -> wasmtime::Result<()> {
        todo!()
    }

    fn width(
        &mut self,
        _self_: Resource<webgpu::GpuTexture>,
    ) -> wasmtime::Result<webgpu::GpuIntegerCoordinateOut> {
        todo!()
    }

    fn height(
        &mut self,
        _self_: Resource<webgpu::GpuTexture>,
    ) -> wasmtime::Result<webgpu::GpuIntegerCoordinateOut> {
        todo!()
    }

    fn depth_or_array_layers(
        &mut self,
        _self_: Resource<webgpu::GpuTexture>,
    ) -> wasmtime::Result<webgpu::GpuIntegerCoordinateOut> {
        todo!()
    }

    fn mip_level_count(
        &mut self,
        _self_: Resource<webgpu::GpuTexture>,
    ) -> wasmtime::Result<webgpu::GpuIntegerCoordinateOut> {
        todo!()
    }

    fn sample_count(
        &mut self,
        _self_: Resource<webgpu::GpuTexture>,
    ) -> wasmtime::Result<webgpu::GpuSize32Out> {
        todo!()
    }

    fn dimension(
        &mut self,
        _self_: Resource<webgpu::GpuTexture>,
    ) -> wasmtime::Result<webgpu::GpuTextureDimension> {
        todo!()
    }

    fn format(
        &mut self,
        _self_: Resource<webgpu::GpuTexture>,
    ) -> wasmtime::Result<webgpu::GpuTextureFormat> {
        todo!()
    }

    fn usage(
        &mut self,
        _self_: Resource<webgpu::GpuTexture>,
    ) -> wasmtime::Result<webgpu::GpuFlagsConstant> {
        todo!()
    }

    fn label(&mut self, _self_: Resource<webgpu::GpuTexture>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuTexture>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }
}

impl webgpu::HostGpuTextureView for HostState {
    fn drop(&mut self, _rep: Resource<wgpu_core::id::TextureViewId>) -> wasmtime::Result<()> {
        Ok(())
    }

    fn label(
        &mut self,
        _self_: Resource<wgpu_core::id::TextureViewId>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<wgpu_core::id::TextureViewId>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }
}

impl webgpu::HostGpuCommandBuffer for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::GpuCommandBuffer>) -> wasmtime::Result<()> {
        // self.web_gpu_host.command_buffers.remove(&rep.rep());
        Ok(())
    }

    fn label(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandBufferId>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandBufferId>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }
}

impl webgpu::HostGpuShaderModule for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::GpuShaderModule>) -> wasmtime::Result<()> {
        // self.web_gpu_host.shaders.remove(&rep.rep());
        Ok(())
    }

    fn get_compilation_info(
        &mut self,
        _self_: Resource<wgpu_core::id::ShaderModuleId>,
    ) -> wasmtime::Result<Resource<webgpu::GpuCompilationInfo>> {
        todo!()
    }

    fn label(
        &mut self,
        _self_: Resource<wgpu_core::id::ShaderModuleId>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<wgpu_core::id::ShaderModuleId>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }
}

impl webgpu::HostGpuRenderPipeline for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::GpuRenderPipeline>) -> wasmtime::Result<()> {
        // TODO:
        Ok(())
    }

    fn label(
        &mut self,
        _self_: Resource<wgpu_core::id::RenderPipelineId>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<wgpu_core::id::RenderPipelineId>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn get_bind_group_layout(
        &mut self,
        _self_: Resource<wgpu_core::id::RenderPipelineId>,
        _index: u32,
    ) -> wasmtime::Result<Resource<webgpu::GpuBindGroupLayout>> {
        todo!()
    }
}

impl webgpu::HostGpuAdapter for HostState {
    fn request_device(
        &mut self,
        adapter: Resource<wgpu_core::id::AdapterId>,
        descriptor: Option<webgpu::GpuDeviceDescriptor>,
    ) -> wasmtime::Result<Resource<webgpu::GpuDevice>> {
        let adapter_id = self.table.get(&adapter).unwrap();

        let device_id = core_result(
            self.instance.adapter_request_device::<crate::Backend>(
                *adapter_id,
                &descriptor
                    .map(|d| d.to_core(&self.table))
                    .unwrap_or_default(),
                None,
                (),
            ),
        )
        .unwrap();

        let daq = self
            .table
            .push_child(
                Device {
                    device: device_id,
                    adapter: *adapter_id,
                },
                &adapter,
            )
            .unwrap();

        Ok(daq)
    }

    fn drop(&mut self, _adapter: Resource<webgpu::GpuAdapter>) -> wasmtime::Result<()> {
        Ok(())
    }

    fn features(
        &mut self,
        _self_: wasmtime::component::Resource<wgpu_core::id::AdapterId>,
    ) -> wasmtime::Result<wasmtime::component::Resource<webgpu::GpuSupportedFeatures>> {
        todo!()
    }

    fn limits(
        &mut self,
        _self_: wasmtime::component::Resource<wgpu_core::id::AdapterId>,
    ) -> wasmtime::Result<wasmtime::component::Resource<webgpu::GpuSupportedLimits>> {
        todo!()
    }

    fn is_fallback_adapter(
        &mut self,
        _self_: wasmtime::component::Resource<wgpu_core::id::AdapterId>,
    ) -> wasmtime::Result<bool> {
        todo!()
    }

    fn request_adapter_info(
        &mut self,
        _self_: wasmtime::component::Resource<wgpu_core::id::AdapterId>,
    ) -> wasmtime::Result<wasmtime::component::Resource<webgpu::GpuAdapterInfo>> {
        todo!()
    }
}

impl webgpu::HostGpuQueue for HostState {
    fn submit(
        &mut self,
        daq: Resource<Device>,
        val: Vec<Resource<webgpu::GpuCommandBuffer>>,
    ) -> wasmtime::Result<()> {
        let command_buffers = val
            .into_iter()
            .map(|buffer| self.table.delete(buffer).unwrap())
            .collect::<Vec<_>>();

        let daq = self.table.get(&daq).unwrap();
        self.instance
            .queue_submit::<crate::Backend>(daq.device, &command_buffers)
            .unwrap();

        Ok(())
    }

    fn drop(&mut self, _rep: Resource<Device>) -> wasmtime::Result<()> {
        // todo!()
        Ok(())
    }

    fn on_submitted_work_done(&mut self, _self_: Resource<Device>) -> wasmtime::Result<()> {
        todo!()
    }

    fn write_buffer(
        &mut self,
        _self_: Resource<Device>,
        _buffer: Resource<webgpu::GpuBuffer>,
        _buffer_offset: webgpu::GpuSize64,
        _data_offset: Option<webgpu::GpuSize64>,
        _data: Resource<webgpu::AllowSharedBufferSource>,
        _size: Option<webgpu::GpuSize64>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn write_texture(
        &mut self,
        _self_: Resource<Device>,
        _destination: webgpu::GpuImageCopyTexture,
        _data: Resource<webgpu::AllowSharedBufferSource>,
        _data_layout: webgpu::GpuImageDataLayout,
        _size: webgpu::GpuExtent3D,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn copy_external_image_to_texture(
        &mut self,
        _self_: Resource<Device>,
        _source: webgpu::GpuImageCopyExternalImage,
        _destination: webgpu::GpuImageCopyTextureTagged,
        _copy_size: webgpu::GpuExtent3D,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn label(&mut self, _self_: Resource<Device>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(&mut self, _self_: Resource<Device>, _label: String) -> wasmtime::Result<()> {
        todo!()
    }
}

impl webgpu::HostGpuCommandEncoder for HostState {
    fn begin_render_pass(
        &mut self,
        command_encoder: Resource<wgpu_core::id::CommandEncoderId>,
        descriptor: webgpu::GpuRenderPassDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuRenderPassEncoder>> {
        let render_pass = wgpu_core::command::RenderPass::new(
            command_encoder.to_core(&self.table),
            &descriptor.to_core(&self.table),
        );

        Ok(self.table.push(render_pass).unwrap())
    }

    fn finish(
        &mut self,
        command_encoder: Resource<wgpu_core::id::CommandEncoderId>,
        descriptor: Option<webgpu::GpuCommandBufferDescriptor>,
    ) -> wasmtime::Result<Resource<webgpu::GpuCommandBuffer>> {
        let command_encoder = self.table.delete(command_encoder).unwrap();
        let command_buffer = core_result(
            self.instance.command_encoder_finish::<crate::Backend>(
                command_encoder,
                &descriptor
                    .map(|d| d.to_core(&self.table))
                    .unwrap_or_default(),
            ),
        )
        .unwrap();
        Ok(self.table.push(command_buffer).unwrap())
    }

    fn drop(&mut self, _rep: Resource<wgpu_core::id::CommandEncoderId>) -> wasmtime::Result<()> {
        Ok(())
    }

    fn begin_compute_pass(
        &mut self,
        command_encoder: Resource<wgpu_core::id::CommandEncoderId>,
        descriptor: Option<webgpu::GpuComputePassDescriptor>,
    ) -> wasmtime::Result<Resource<webgpu::GpuComputePassEncoder>> {
        let compute_pass = wgpu_core::command::ComputePass::new(
            command_encoder.to_core(&self.table),
            &descriptor
                .map(|d| d.to_core(&self.table))
                .unwrap_or_default(),
        );

        Ok(self.table.push(compute_pass).unwrap())
    }

    fn copy_buffer_to_buffer(
        &mut self,
        command_encoder: Resource<wgpu_core::id::CommandEncoderId>,
        source: Resource<webgpu::GpuBuffer>,
        source_offset: webgpu::GpuSize64,
        destination: Resource<webgpu::GpuBuffer>,
        destination_offset: webgpu::GpuSize64,
        size: webgpu::GpuSize64,
    ) -> wasmtime::Result<()> {
        //let compute_pass = command_encoder.to_core(&self.table);
        let encoder = self.table.get(&command_encoder).unwrap();
        let source = self.table.get(&source).unwrap();
        let destination = self.table.get(&destination).unwrap();
       
        let err = self.instance.command_encoder_copy_buffer_to_buffer::<crate::Backend>(
            *encoder, 
            source.buf,
            source_offset,
            destination.buf,
            destination_offset,
            size
        );
        println!("copy : {:?}", err);
        Ok(())
    }

    fn copy_buffer_to_texture(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
        _source: webgpu::GpuImageCopyBuffer,
        _destination: webgpu::GpuImageCopyTexture,
        _copy_size: webgpu::GpuExtent3D,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn copy_texture_to_buffer(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
        _source: webgpu::GpuImageCopyTexture,
        _destination: webgpu::GpuImageCopyBuffer,
        _copy_size: webgpu::GpuExtent3D,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn copy_texture_to_texture(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
        _source: webgpu::GpuImageCopyTexture,
        _destination: webgpu::GpuImageCopyTexture,
        _copy_size: webgpu::GpuExtent3D,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn clear_buffer(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
        _buffer: Resource<webgpu::GpuBuffer>,
        _offset: Option<webgpu::GpuSize64>,
        _size: Option<webgpu::GpuSize64>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn resolve_query_set(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
        _query_set: Resource<webgpu::GpuQuerySet>,
        _first_query: webgpu::GpuSize32,
        _query_count: webgpu::GpuSize32,
        _destination: Resource<webgpu::GpuBuffer>,
        _destination_offset: webgpu::GpuSize64,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn label(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn push_debug_group(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
        _group_label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn pop_debug_group(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn insert_debug_marker(
        &mut self,
        _self_: Resource<wgpu_core::id::CommandEncoderId>,
        _marker_label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }
}

impl webgpu::HostGpuRenderPassEncoder for HostState {
    fn set_pipeline(
        &mut self,
        render_pass: Resource<wgpu_core::command::RenderPass>,
        pipeline: Resource<webgpu::GpuRenderPipeline>,
    ) -> wasmtime::Result<()> {
        let pipeline = pipeline.to_core(&self.table);
        let render_pass = self.table.get_mut(&render_pass).unwrap();
        wgpu_core::command::render_ffi::wgpu_render_pass_set_pipeline(render_pass, pipeline);
        Ok(())
    }

    fn draw(
        &mut self,
        cwr: Resource<wgpu_core::command::RenderPass>,
        vertex_count: webgpu::GpuSize32,
        instance_count: webgpu::GpuSize32,
        first_vertex: webgpu::GpuSize32,
        first_instance: webgpu::GpuSize32,
    ) -> wasmtime::Result<()> {
        let cwr = self.table.get_mut(&cwr).unwrap();

        wgpu_core::command::render_ffi::wgpu_render_pass_draw(
            cwr,
            vertex_count,
            instance_count,
            first_vertex,
            first_instance,
        );

        Ok(())
    }

    fn end(
        &mut self,
        rpass: Resource<wgpu_core::command::RenderPass>,
        non_standard_encoder: Resource<wgpu_core::id::CommandEncoderId>,
    ) -> wasmtime::Result<()> {
        // use this instead of non_standard_present? Ask on ...

        let rpass = self.table.delete(rpass).unwrap();
        let encoder = self.table.get(&non_standard_encoder).unwrap();
        self.instance
            .command_encoder_run_render_pass::<crate::Backend>(*encoder, &rpass)
            .unwrap();
        Ok(())
    }

    fn drop(&mut self, cwr: Resource<wgpu_core::command::RenderPass>) -> wasmtime::Result<()> {
        self.table.delete(cwr).unwrap();
        Ok(())
    }

    fn set_viewport(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _x: f32,
        _y: f32,
        _width: f32,
        _height: f32,
        _min_depth: f32,
        _max_depth: f32,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_scissor_rect(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _x: webgpu::GpuIntegerCoordinate,
        _y: webgpu::GpuIntegerCoordinate,
        _width: webgpu::GpuIntegerCoordinate,
        _height: webgpu::GpuIntegerCoordinate,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_blend_constant(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _color: webgpu::GpuColor,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_stencil_reference(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _reference: webgpu::GpuStencilValue,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn begin_occlusion_query(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _query_index: webgpu::GpuSize32,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn end_occlusion_query(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn execute_bundles(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _bundles: Vec<Resource<webgpu::GpuRenderBundle>>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn label(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn push_debug_group(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _group_label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn pop_debug_group(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn insert_debug_marker(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _marker_label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_bind_group(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _index: webgpu::GpuIndex32,
        _bind_group: Resource<webgpu::GpuBindGroup>,
        _dynamic_offsets: Option<Vec<webgpu::GpuBufferDynamicOffset>>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_index_buffer(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _buffer: Resource<webgpu::GpuBuffer>,
        _index_format: webgpu::GpuIndexFormat,
        _offset: webgpu::GpuSize64,
        _size: webgpu::GpuSize64,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_vertex_buffer(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _slot: webgpu::GpuIndex32,
        _buffer: Resource<webgpu::GpuBuffer>,
        _offset: webgpu::GpuSize64,
        _size: webgpu::GpuSize64,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn draw_indexed(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _index_count: webgpu::GpuSize32,
        _instance_count: webgpu::GpuSize32,
        _first_index: webgpu::GpuSize32,
        _base_vertex: webgpu::GpuSignedOffset32,
        _first_instance: webgpu::GpuSize32,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn draw_indirect(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _indirect_buffer: Resource<webgpu::GpuBuffer>,
        _indirect_offset: webgpu::GpuSize64,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn draw_indexed_indirect(
        &mut self,
        _self_: Resource<wgpu_core::command::RenderPass>,
        _indirect_buffer: Resource<webgpu::GpuBuffer>,
        _indirect_offset: webgpu::GpuSize64,
    ) -> wasmtime::Result<()> {
        todo!()
    }
}

impl webgpu::HostGpuUncapturedErrorEvent for HostState {
    fn new(
        &mut self,
        _type_: String,
        _gpu_uncaptured_error_event_init_dict: webgpu::GpuUncapturedErrorEventInit,
    ) -> wasmtime::Result<Resource<webgpu::GpuUncapturedErrorEvent>> {
        todo!()
    }

    fn error(
        &mut self,
        _self_: Resource<webgpu::GpuUncapturedErrorEvent>,
    ) -> wasmtime::Result<Resource<webgpu::GpuError>> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuUncapturedErrorEvent>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuInternalError for HostState {
    fn new(&mut self, _message: String) -> wasmtime::Result<Resource<webgpu::GpuInternalError>> {
        todo!()
    }

    fn message(&mut self, _self_: Resource<webgpu::GpuInternalError>) -> wasmtime::Result<String> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuInternalError>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuOutOfMemoryError for HostState {
    fn new(&mut self, _message: String) -> wasmtime::Result<Resource<webgpu::GpuOutOfMemoryError>> {
        todo!()
    }

    fn message(
        &mut self,
        _self_: Resource<webgpu::GpuOutOfMemoryError>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuOutOfMemoryError>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuValidationError for HostState {
    fn new(&mut self, _message: String) -> wasmtime::Result<Resource<webgpu::GpuValidationError>> {
        todo!()
    }

    fn message(
        &mut self,
        _self_: Resource<webgpu::GpuValidationError>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuValidationError>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuError for HostState {
    fn message(&mut self, _self_: Resource<webgpu::GpuError>) -> wasmtime::Result<String> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuError>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuDeviceLostInfo for HostState {
    fn reason(
        &mut self,
        _self_: Resource<webgpu::GpuDeviceLostInfo>,
    ) -> wasmtime::Result<webgpu::GpuDeviceLostReason> {
        todo!()
    }

    fn message(&mut self, _self_: Resource<webgpu::GpuDeviceLostInfo>) -> wasmtime::Result<String> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuDeviceLostInfo>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuCanvasContext for HostState {
    fn canvas(
        &mut self,
        _self_: Resource<webgpu::GpuCanvasContext>,
    ) -> wasmtime::Result<webgpu::HtmlCanvasElementOrOffscreenCanvas> {
        todo!()
    }

    fn configure(
        &mut self,
        _self_: Resource<webgpu::GpuCanvasContext>,
        _configuration: webgpu::GpuCanvasConfiguration,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn unconfigure(&mut self, _self_: Resource<webgpu::GpuCanvasContext>) -> wasmtime::Result<()> {
        todo!()
    }

    fn get_current_texture(
        &mut self,
        _self_: Resource<webgpu::GpuCanvasContext>,
    ) -> wasmtime::Result<Resource<webgpu::GpuTexture>> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuCanvasContext>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuRenderBundle for HostState {
    fn label(&mut self, _self_: Resource<webgpu::GpuRenderBundle>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundle>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuRenderBundle>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuComputePassEncoder for HostState {
    fn set_pipeline(
        &mut self,
        compute_pass: Resource<wgpu_core::command::ComputePass>,
        pipeline: Resource<webgpu::GpuComputePipeline>,
    ) -> wasmtime::Result<()> {
        let pipeline = pipeline.to_core(&self.table);
        let computer_pass = self.table.get_mut(&compute_pass).unwrap();
        wgpu_core::command::compute_ffi::wgpu_compute_pass_set_pipeline(computer_pass, pipeline.pipe);
        Ok(())
    }

    fn dispatch_workgroups(
        &mut self,
        compute_pass: Resource<wgpu_core::command::ComputePass>,
        _workgroup_count_x: webgpu::GpuSize32,
        _workgroup_count_y: Option<webgpu::GpuSize32>,
        _workgroup_count_z: Option<webgpu::GpuSize32>,
    ) -> wasmtime::Result<()> {
        let computer_pass = self.table.get_mut(&compute_pass).unwrap();
        wgpu_core::command::compute_ffi::wgpu_compute_pass_dispatch_workgroups(computer_pass, _workgroup_count_x, 1 ,1);
        Ok(())
    }

    fn dispatch_workgroups_indirect(
        &mut self,
        _self_: Resource<webgpu::GpuComputePassEncoder>,
        _indirect_buffer: Resource<webgpu::GpuBuffer>,
        _indirect_offset: webgpu::GpuSize64,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn end(&mut self, 
        rpass: Resource<wgpu_core::command::ComputePass> ,
        non_standard_encoder: Resource<wgpu_core::id::CommandEncoderId>) -> wasmtime::Result<()> {
        let rpass = self.table.delete(rpass).unwrap();
        let encoder = self.table.get(&non_standard_encoder).unwrap();
        self.instance
                .command_encoder_run_compute_pass::<crate::Backend>(*encoder, &rpass)
                .unwrap();
        //wgpu_core::command::compute_ffi::wgpu_compute_pass_end();
        Ok(())
    }


    fn label(
        &mut self,
        _self_: Resource<webgpu::GpuComputePassEncoder>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuComputePassEncoder>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn push_debug_group(
        &mut self,
        _self_: Resource<webgpu::GpuComputePassEncoder>,
        _group_label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn pop_debug_group(
        &mut self,
        _self_: Resource<webgpu::GpuComputePassEncoder>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn insert_debug_marker(
        &mut self,
        _self_: Resource<webgpu::GpuComputePassEncoder>,
        _marker_label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_bind_group(
        &mut self,
        compute_pass: Resource<wgpu_core::command::ComputePass>,
        index: webgpu::GpuIndex32,
        bind_group: Resource<webgpu::GpuBindGroup>,
        _: Option<Vec<webgpu::GpuBufferDynamicOffset>>,
    ) -> wasmtime::Result<()> {
        
        let bind_group_desc = *self.table.get(&bind_group).unwrap();
        let computer_pass = self.table.get_mut(&compute_pass).unwrap();
        
        let null_ptr: *const u32 = std::ptr::null();
        unsafe {
            wgpu_core::command::compute_ffi::wgpu_compute_pass_set_bind_group(computer_pass, index, bind_group_desc, null_ptr, 0);
        }
        Ok(())
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuComputePassEncoder>) -> wasmtime::Result<()> {
        //todo!()
        Ok(())
    }
}
impl webgpu::HostGpuPipelineError for HostState {
    fn new(
        &mut self,
        _message: Option<String>,
        _options: webgpu::GpuPipelineErrorInit,
    ) -> wasmtime::Result<Resource<webgpu::GpuPipelineError>> {
        todo!()
    }

    fn reason(
        &mut self,
        _self_: Resource<webgpu::GpuPipelineError>,
    ) -> wasmtime::Result<webgpu::GpuPipelineErrorReason> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuPipelineError>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuCompilationMessage for HostState {
    fn message(
        &mut self,
        _self_: Resource<webgpu::GpuCompilationMessage>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn type_(
        &mut self,
        _self_: Resource<webgpu::GpuCompilationMessage>,
    ) -> wasmtime::Result<webgpu::GpuCompilationMessageType> {
        todo!()
    }

    fn line_num(
        &mut self,
        _self_: Resource<webgpu::GpuCompilationMessage>,
    ) -> wasmtime::Result<u64> {
        todo!()
    }

    fn line_pos(
        &mut self,
        _self_: Resource<webgpu::GpuCompilationMessage>,
    ) -> wasmtime::Result<u64> {
        todo!()
    }

    fn offset(&mut self, _self_: Resource<webgpu::GpuCompilationMessage>) -> wasmtime::Result<u64> {
        todo!()
    }

    fn length(&mut self, _self_: Resource<webgpu::GpuCompilationMessage>) -> wasmtime::Result<u64> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuCompilationMessage>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuCompilationInfo for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::GpuCompilationInfo>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuQuerySet for HostState {
    fn destroy(&mut self, _self_: Resource<webgpu::GpuQuerySet>) -> wasmtime::Result<()> {
        todo!()
    }

    fn type_(
        &mut self,
        _self_: Resource<webgpu::GpuQuerySet>,
    ) -> wasmtime::Result<webgpu::GpuQueryType> {
        todo!()
    }

    fn count(
        &mut self,
        _self_: Resource<webgpu::GpuQuerySet>,
    ) -> wasmtime::Result<webgpu::GpuSize32Out> {
        todo!()
    }

    fn label(&mut self, _self_: Resource<webgpu::GpuQuerySet>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuQuerySet>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuQuerySet>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuRenderBundleEncoder for HostState {
    fn finish(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _descriptor: Option<webgpu::GpuRenderBundleDescriptor>,
    ) -> wasmtime::Result<Resource<webgpu::GpuRenderBundle>> {
        todo!()
    }

    fn label(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn push_debug_group(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _group_label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn pop_debug_group(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn insert_debug_marker(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _marker_label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_bind_group(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _index: webgpu::GpuIndex32,
        _bind_group: Resource<webgpu::GpuBindGroup>,
        _dynamic_offsets: Option<Vec<webgpu::GpuBufferDynamicOffset>>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_pipeline(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _pipeline: Resource<wgpu_core::id::RenderPipelineId>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_index_buffer(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _buffer: Resource<webgpu::GpuBuffer>,
        _index_format: webgpu::GpuIndexFormat,
        _offset: Option<webgpu::GpuSize64>,
        _size: Option<webgpu::GpuSize64>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn set_vertex_buffer(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _slot: webgpu::GpuIndex32,
        _buffer: Resource<webgpu::GpuBuffer>,
        _offset: Option<webgpu::GpuSize64>,
        _size: Option<webgpu::GpuSize64>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn draw(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _vertex_count: webgpu::GpuSize32,
        _instance_count: Option<webgpu::GpuSize32>,
        _first_vertex: Option<webgpu::GpuSize32>,
        _first_instance: Option<webgpu::GpuSize32>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn draw_indexed(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _index_count: webgpu::GpuSize32,
        _instance_count: Option<webgpu::GpuSize32>,
        _first_index: Option<webgpu::GpuSize32>,
        _base_vertex: Option<webgpu::GpuSignedOffset32>,
        _first_instance: Option<webgpu::GpuSize32>,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn draw_indirect(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _indirect_buffer: Resource<webgpu::GpuBuffer>,
        _indirect_offset: webgpu::GpuSize64,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn draw_indexed_indirect(
        &mut self,
        _self_: Resource<webgpu::GpuRenderBundleEncoder>,
        _indirect_buffer: Resource<webgpu::GpuBuffer>,
        _indirect_offset: webgpu::GpuSize64,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuRenderBundleEncoder>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuComputePipeline for HostState {
    fn label(&mut self, _self_: Resource<webgpu::GpuComputePipeline>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuComputePipeline>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn get_bind_group_layout(
        &mut self,
        pipeline: Resource<webgpu::GpuComputePipeline>,
        index: u32,
    ) -> wasmtime::Result<Resource<webgpu::GpuBindGroupLayout>> {
        let p = self.table.get(&pipeline).unwrap();
        //let pipe = p.device.get_compute_pipeline(p.pipe).unwrap();

        let layout = core_result(
            self.instance.compute_pipeline_get_bind_group_layout::<crate::Backend>(
                p.pipe,
                index, 
                ()
            )).unwrap();
        Ok(self.table.push(layout).unwrap())
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuComputePipeline>) -> wasmtime::Result<()> {
        //todo!()
        Ok(())
    }
}
impl webgpu::HostGpuBindGroup for HostState {
    fn label(&mut self, _self_: Resource<webgpu::GpuBindGroup>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuBindGroup>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuBindGroup>) -> wasmtime::Result<()> {
        //todo!()
        Ok(())
    }
}
impl webgpu::HostGpuPipelineLayout for HostState {
    fn label(&mut self, _self_: Resource<webgpu::GpuPipelineLayout>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuPipelineLayout>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuPipelineLayout>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuBindGroupLayout for HostState {
    fn label(&mut self, _self_: Resource<webgpu::GpuBindGroupLayout>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuBindGroupLayout>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuBindGroupLayout>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuExternalTexture for HostState {
    fn label(&mut self, _self_: Resource<webgpu::GpuExternalTexture>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuExternalTexture>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuExternalTexture>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuSampler for HostState {
    fn label(&mut self, _self_: Resource<webgpu::GpuSampler>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuSampler>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuSampler>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuBuffer for HostState {
    fn clone(
        &mut self,
        buffer: Resource<webgpu::GpuBuffer>,
    ) -> wasmtime::Result<Resource<webgpu::GpuBuffer>> {
        let res: &Buffer = self.table.get(&buffer).unwrap();
        let daq = self
            .table
            .push(
                Buffer { buf: res.buf , dev: res.dev}
            )
            .unwrap();
        //let wres = core_result((buf, None)).unwrap();
        Ok(daq)
    }

    fn size(
        &mut self,
        _self_: Resource<webgpu::GpuBuffer>,
    ) -> wasmtime::Result<webgpu::GpuSize64Out> {
        todo!()
    }

    fn usage(
        &mut self,
        _self_: Resource<webgpu::GpuBuffer>,
    ) -> wasmtime::Result<webgpu::GpuFlagsConstant> {
        todo!()
    }

    fn map_state(
        &mut self,
        _self_: Resource<webgpu::GpuBuffer>,
    ) -> wasmtime::Result<webgpu::GpuBufferMapState> {
        todo!()
    }

  

    fn map_async(
        &mut self,
        buf: Resource<webgpu::GpuBuffer>,
        _mode: webgpu::GpuMapModeFlags,
        offset: Option<webgpu::GpuSize64>,
        size: Option<webgpu::GpuSize64>,
    ) -> wasmtime::Result<()> {
       let buffer  = self.table.get(&buf).unwrap();
       
       
        //let (sender, receiver) = flume::bounded(1);
        let operation = wgpu_core::resource::BufferMapOperation {
            host:  wgpu_core::device::HostMap::Read,
            callback: wgpu_core::resource::BufferMapCallback::from_rust(Box::new(map_callback)),
        };
        self.instance.buffer_map_async::<crate::Backend>(
                buffer.buf,
                offset.expect("") .. offset.expect("")+size.expect("") as wgpu_types::BufferAddress,
                operation).unwrap();
        
        self.instance.device_poll::<crate::Backend>(buffer.dev, wgpu_types::Maintain::Wait).unwrap();
        let (data, size) = self.instance.buffer_get_mapped_range::<crate::Backend>(
            buffer.buf,
            offset.expect(""),
            size,
        ).unwrap();
        let contents = unsafe { std::slice::from_raw_parts(data, size as usize) };
        //let result: Vec<f32> = bytemuck::cast_slice(contents).to_vec();
        //println!("res : {:?}\n", result.len());
        Ok(())
    }

    fn get_mapped_range(
        &mut self,
        _self_: Resource<webgpu::GpuBuffer>,
        _offset: webgpu::GpuSize64,
        _size: webgpu::GpuSize64,
    ) -> wasmtime::Result<Resource<webgpu::ArrayBuffer>> {
        todo!()
    }

    fn unmap(&mut self, _self_: Resource<webgpu::GpuBuffer>) -> wasmtime::Result<()> {
        todo!()
    }

    fn destroy(&mut self, _self_: Resource<webgpu::GpuBuffer>) -> wasmtime::Result<()> {
        todo!()
    }

    fn label(&mut self, _self_: Resource<webgpu::GpuBuffer>) -> wasmtime::Result<String> {
        todo!()
    }

    fn set_label(
        &mut self,
        _self_: Resource<webgpu::GpuBuffer>,
        _label: String,
    ) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuBuffer>) -> wasmtime::Result<()> {
        //todo!()
        Ok(())
    }
}
impl webgpu::HostGpu for HostState {
    fn request_adapter(
        &mut self,
        _self_: Resource<webgpu::Gpu>,
        _options: Option<webgpu::GpuRequestAdapterOptions>,
    ) -> wasmtime::Result<Resource<wgpu_core::id::AdapterId>> {
        let adapter = self
            .instance
            .request_adapter(
                &Default::default(),
                wgpu_core::instance::AdapterInputs::Mask(wgpu_types::Backends::all(), |_| ()),
            )
            .unwrap();
        Ok(self.table.push(adapter).unwrap())
    }

    fn get_preferred_canvas_format(
        &mut self,
        _self_: Resource<webgpu::Gpu>,
    ) -> wasmtime::Result<webgpu::GpuTextureFormat> {
        todo!()
    }

    fn wgsl_language_features(
        &mut self,
        _self_: Resource<webgpu::Gpu>,
    ) -> wasmtime::Result<Resource<webgpu::WgslLanguageFeatures>> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::Gpu>) -> wasmtime::Result<()> {
        Ok(())
        //todo!()
    }
}
impl webgpu::HostGpuAdapterInfo for HostState {
    fn vendor(&mut self, _self_: Resource<webgpu::GpuAdapterInfo>) -> wasmtime::Result<String> {
        todo!()
    }

    fn architecture(
        &mut self,
        _self_: Resource<webgpu::GpuAdapterInfo>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn device(&mut self, _self_: Resource<webgpu::GpuAdapterInfo>) -> wasmtime::Result<String> {
        todo!()
    }

    fn description(
        &mut self,
        _self_: Resource<webgpu::GpuAdapterInfo>,
    ) -> wasmtime::Result<String> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuAdapterInfo>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostWgslLanguageFeatures for HostState {
    fn has(
        &mut self,
        _self_: Resource<webgpu::WgslLanguageFeatures>,
        _key: String,
    ) -> wasmtime::Result<bool> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::WgslLanguageFeatures>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuSupportedFeatures for HostState {
    fn has(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedFeatures>,
        _key: String,
    ) -> wasmtime::Result<bool> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuSupportedFeatures>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostGpuSupportedLimits for HostState {
    fn max_texture_dimension1_d(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_texture_dimension2_d(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_texture_dimension3_d(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_texture_array_layers(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_bind_groups(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_bind_groups_plus_vertex_buffers(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_bindings_per_bind_group(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_dynamic_uniform_buffers_per_pipeline_layout(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_dynamic_storage_buffers_per_pipeline_layout(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_sampled_textures_per_shader_stage(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_samplers_per_shader_stage(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_storage_buffers_per_shader_stage(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_storage_textures_per_shader_stage(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_uniform_buffers_per_shader_stage(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_uniform_buffer_binding_size(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u64> {
        todo!()
    }

    fn max_storage_buffer_binding_size(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u64> {
        todo!()
    }

    fn min_uniform_buffer_offset_alignment(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn min_storage_buffer_offset_alignment(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_vertex_buffers(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_buffer_size(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u64> {
        todo!()
    }

    fn max_vertex_attributes(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_vertex_buffer_array_stride(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_inter_stage_shader_components(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_inter_stage_shader_variables(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_color_attachments(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_color_attachment_bytes_per_sample(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_compute_workgroup_storage_size(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_compute_invocations_per_workgroup(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_compute_workgroup_size_x(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_compute_workgroup_size_y(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_compute_workgroup_size_z(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn max_compute_workgroups_per_dimension(
        &mut self,
        _self_: Resource<webgpu::GpuSupportedLimits>,
    ) -> wasmtime::Result<u32> {
        todo!()
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuSupportedLimits>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostAllowSharedBufferSource for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::AllowSharedBufferSource>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostPredefinedColorSpace for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::PredefinedColorSpace>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostEventHandler for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::EventHandler>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostOffscreenCanvas for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::OffscreenCanvas>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostHtmlCanvasElement for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::HtmlCanvasElement>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostVideoFrame for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::VideoFrame>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostHtmlVideoElement for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::HtmlVideoElement>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostHtmlImageElement for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::HtmlImageElement>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostImageData for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::ImageData>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostImageBitmap for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::ImageBitmap>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostArrayBuffer for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::ArrayBuffer>) -> wasmtime::Result<()> {
        todo!()
    }
}
impl webgpu::HostUint32Array for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::Uint32Array>) -> wasmtime::Result<()> {
        todo!()
    }
}

fn core_result<I, E>(
    (id, error): (wgpu_core::id::Id<I>, Option<E>),
) -> Result<wgpu_core::id::Id<I>, E> {
    match error {
        Some(error) => Err(error),
        None => Ok(id),
    }
}
