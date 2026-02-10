trait Node{
    fn ensure_pipeline(&mut self, device: &wgpu::Device);
    fn set_resources(&mut self, device: &wgpu::Device);
}