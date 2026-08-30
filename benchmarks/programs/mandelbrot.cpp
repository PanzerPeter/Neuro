// Scalar floating-point hot loop: Mandelbrot escape counts over a 1000x1000
// grid. Almost pure f64 arithmetic in a tight nested loop, so it measures how
// well locals are kept in registers and how well the inner loop is optimized.

#include <cstdio>
int mandel(int w,int h,int max_iter){
    int total=0;
    for(int py=0;py<h;py++){
        for(int px=0;px<w;px++){
            double x0=(double)px/(double)w*3.5-2.5;
            double y0=(double)py/(double)h*2.0-1.0;
            double x=0.0,y=0.0; int i=0;
            while(i<max_iter){
                double xx=x*x, yy=y*y;
                if(xx+yy>4.0) break;
                double xt=xx-yy+x0;
                y=2.0*x*y+y0; x=xt; i++;
            }
            total+=i;
        }
    }
    return total;
}
int main(){ printf("mandel = %d\n", mandel(1000,1000,400)); }
