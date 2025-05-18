#!/bin/bash
cd brahmGAN || exit
echo "Daily activity on $(date '+%Y-%m-%d')" >> log.txt
git add log.txt
git commit -m "Daily activity on $(date '+%Y-%m-%d')"
git push origin main
daily1
daily2..
daily3
