# Rust Watch Project

This is a WIP project to make a smart watch using embedded Rust on an esp32-s3

## Design Overview:

### Hardware Overview:

#### BOM (Subject to change):
- 1x, Display + Board: waveshare ESP32-S3-Touch-AMOLED-1.43
  
  Purchase at: https://www.amazon.com/Waveshare-Development-Interface-Accelerometer-Gyroscope/dp/B0DT9PSDW6/ref=sr_1_2
  
  Information/Reference: https://www.waveshare.com/wiki/ESP32-S3-Touch-AMOLED-1.43.

  Notes: The Waveshare OLED esp32-s3 board was chosen in part pimarily due to the processing power and psram of the s3 and due to the quality, size, and resolution of the OLED display and the additional features included in the devboard.
  Waveshare additionally includes arduiono and esp-idf examples in C or C++ for reference.

- 1x, Battery: DC 3.7V 500mAh 503030 Rechargeable Lithium Polymer Battery

  Purchase at: https://www.amazon.com/gp/product/B0CP24RVYK/ref=ewc_pr_img_1

  Information/Reference: https://www.amazon.com/gp/product/B0CP24RVYK/ref=ewc_pr_img_1

  Notes: Chosen due to dimensions, voltage, capacity, and jst 1.25 connector already included.
  
- 1x, Rotary Encoder: PES12-40S-N0024
  
  Purchase at: https://www.mouser.com/ProductDetail/Bourns/PES12-40S-N0024?qs=9fn1gpisni7PBzk9dl6VPg%3D%3D&srsltid=AfmBOor_oTmeC9qw_lexNLodGN5nypOgnqC5zxftrMN7JiiiWoYjBkBL
  
  Information/Reference: https://www.bourns.com/docs/Product-Datasheets/PES12.pdf
  
  Notes: Chosen due to availability, height, torque, and detents. Very possible future iteration may change the encoder.
- 2x, Buttons: TE Connectivity ALCOSWITCH Switches 1977067-1
  
  Purchase at: https://www.digikey.com/en/products/detail/te-connectivity-alcoswitch-switches/1977067-1/5596904
  
  Information/Reference: https://www.te.com/commerce/DocumentDelivery/DDEController?Action=srchrtrv&DocNm=1977067&DocType=Customer+Drawing&DocLang=English&DocFormat=pdf&PartCntxt=1977067-1

  Notes: Chosen due to availability and height.
- 2x, 10 kOhms ±1% 0.125W, 1/8W Chip Resistor 0805: Used RC0805FR-0710KP, others may work

  Purchase at: https://www.digikey.com/en/products/detail/yageo/RC0805FR-0710KP/4935334

  Notes: No particular reason chosen beyond needing an smd 10kOhm resistor.
  Information/Reference: https://www.yageogroup.com/content/datasheet/asset/file/PYU-RC_51_ROHS_P
- 2x, 0.1 µF ±10% 50V Ceramic Capacitor: Used CL21B104KBCNNNC, others may work

  Purchase at:  https://www.digikey.com/en/products/detail/samsung-electro-mechanics/CL21B104KBCNNNC/3886661

  Information/Reference: https://mm.digikey.com/Volume0/opasdata/d220001/medias/docus/609/CL21B104KBCNNN_Spec.pdf

  Notes: No particular reason chosen beyond needing an smd 0.1 uF capacitor.
- 6x, 24 awg wire at 2mm length
  
  Purchase at: https://www.amazon.com/dp/B07CJYSL2T

  Information/Reference: includes U shape wires, 24 AWG, male to male, wire length 2 mm, 5 mm, 7 mm, 10 mm, 12 mm, 15 mm, 17 mm, 20 mm, 22 mm, 25 mm, 50 mm, 75 mm, 100 mm, 125 mm
- 1x Custom PCB for Watch Inputs
  
  Information/Reference:   Altiums Files located in the repository under Watch_Input_PCB, *Errors/Warnings related to missing schematic libraries may show up, associated altium libraries are not included,
  and should be downloaded from the purchase/information sources above.

  Notes: Altium Designer version 25.8.1 utilized, due to an altium bug, when reproducing the gerberfiles, in the project output folder delete all mechanical layers (.GM#) and keepout layer (.GKO) (messes up the board size).

  Pre-created Gerber Files for Manufacturing located at Watch_Input_PCB/Project Outputs for Watch_Input_PCB.zip

- 1x Physical Housing (incomplete):

  Information/Reference: Fusion 360 models locatied at Watch_Housing_Models.

  Notes: Only the core of the housing has been modeled. The housing has not physically been printed and tested yet, unknown if the model would work or be assembleable in real life. The end goal is for the housing to be wrist mountable.     The rotary encoder ideally should be able to be used through the rotating bezel with a three bevel gear train, the buttons should connect to some button stubs. The wrist connector and base should include a spring locked mechanism to      allow the core to pop up like the Ben 10 Omnitrix Watch.

  #### PCB Schematic (V0):
  
  <img width="890" height="704" alt="image" src="https://github.com/user-attachments/assets/92232321-a05d-4db2-b031-d550acabcd80" />
  
  #### PCB (V0):
  
  <img width="558" height="445" alt="image" src="https://github.com/user-attachments/assets/6c103cb8-8472-4610-b414-a6963d79f5b3" />

  #### Physical Housing (incomplete):

  Section View:
  
  <img width="534" height="329" alt="image" src="https://github.com/user-attachments/assets/30da1533-3aac-428e-8af2-7b9b4dcba1ae" />

  Full View (isometric):

  <img width="544" height="509" alt="image" src="https://github.com/user-attachments/assets/27e37163-5b85-4623-951b-d487718098e8" />


  #### Todo:
    - Testing of core housing
    - Modeling and testing of base+wrist mount housing
    - Addition of additional features beyond simple 3 inputs

### Software Overview:




  
