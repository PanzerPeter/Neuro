# Scalar floating-point hot loop: Mandelbrot escape counts over a 1000x1000
# grid. Almost pure f64 arithmetic in a tight nested loop, so it measures how
# well locals are kept in registers and how well the inner loop is optimized.

def mandel(w,h,max_iter):
    total=0
    for py in range(h):
        for px in range(w):
            x0=px/w*3.5-2.5
            y0=py/h*2.0-1.0
            x=0.0;y=0.0;i=0
            while i<max_iter:
                xx=x*x;yy=y*y
                if xx+yy>4.0: break
                xt=xx-yy+x0
                y=2.0*x*y+y0;x=xt;i+=1
            total+=i
    return total
print("mandel =", mandel(1000,1000,400))
