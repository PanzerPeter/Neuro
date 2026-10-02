# The gpu_relax benchmark with PyTorch on the GPU: the same 200 float32 steps
# `y = (y + x) * h` on tensors that stay on the device, and the same checksum of the
# result brought back to the host.

import torch

n = 2048
i = torch.arange(n).reshape(-1, 1)
j = torch.arange(n).reshape(1, -1)
x = ((i * 7 + j * 3) % 17).float().cuda()
y = torch.zeros(n, n, device="cuda")
for _ in range(200):
    y = (y + x) * 0.5
host = y.cpu()
print(f"checksum = {int((host * 1024.0).to(torch.int64).sum())}")
