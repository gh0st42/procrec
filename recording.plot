set autoscale

set ylabel "CPU %"  tc lt 3
set y2label "Memory (kB)"  tc lt 4
set xlabel "Times (s)"

set ytic auto
set y2tic auto

set yr[0:100]
set y2r[0:]
set xtics auto

set key right center # legend placement
unset key 
plot    filename using 1:5 title "CPU" with l lt 3 lw 2, \
        "" using 1:7 title "RSS" with l lt 4 lw 2 axes x1y2