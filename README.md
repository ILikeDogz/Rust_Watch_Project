# Rust Watch Project

This is a WIP project to make a smart watch programmed in bare metal (no std) embedded Rust from "scratch" on an esp32-s3. 

Demo Video:

https://www.youtube.com/shorts/WcdTKyUyNlw

Images: 

<img width="1008" height="783" alt="image" src="https://github.com/user-attachments/assets/4c80d549-ff28-4cfd-b0a6-e240a691f92d" />

<img width="1008" height="954" alt="image" src="https://github.com/user-attachments/assets/24207fd4-6e11-484b-a1d1-8b972f070628" />


## Overview:
The Watch project features a 1.43 in OLED display apart of an esp32-s3 + display devboard from waveshare. There are 3 physical inputs, 2 buttons and a rotary encoder, and the imu which acts as a secret input for only one menu, intending to work as a fake button due to the impracticality of including a physical button under the watch. The left button is set up to be the back button, and the right button is the select button. The rotary encoder acts as a left or right controller allowing for fast switching through menus. The watch currently features 3 main fully functional "apps". Low power mode for the watch can be activated through holding down the back button. Pressing the select button will wake the watch up after entering low power mode (restarting the watch). Physical Housing is still a WIP, not yet fully modeled and multiple printing issues.

The inspiration to use an esp32 + display devboard for a watch came from this video (This had no other contribution to the design):

https://www.youtube.com/watch?v=E5cJF_3hY-w

The inspiration for the physical housing comes from:

https://ben10.fandom.com/wiki/Omnitrix_(Original)

The examples in C and C++ provided by Waveshare acted as one of the main references for a lot of the driver level software:

https://www.waveshare.com/wiki/ESP32-S3-Touch-AMOLED-1.43

The decision to do it entirely in Rust was a result of my own personal hatred of CMAKE and Arduino IDE, and a love of Rust, being the first programming language I set out to learn anything beyond basics of if, else, loops.

The choice for physical inputs instead of touch screen input, was due to liking the feel of analog inputs, and to better replicate features from the Omnitrix watch.

### App 1: Omnitrix
This is the default app of the watch, modeled off the functionality of the omnitrix from Ben 10. The select button enters the app, initially loading the first of 10 alien images. The rotary encoder can be used to quickly navigate the 10 different available aliens (preselected based on my favorites). Through physically smashing (recommended lightly) the watch, the user is able to select the Alien, which then plays an animation of a dna spiral spinning.

#### Todo/Addition Ideas:
- Include a speaker playing a sound to indicate transformation

### App 2: Time
This is the second app of the watch, this includes both an analog and digital clock with a preloaded background from Cyberpunk Edgerunners, and is themed based on the anime. When selected the analog clock is the page shown first, showing an animated analog clock based on a 12 hour time. Using the rotary encoder, the user can switch to the digital clock, which is based on 24 hour time. The time can be adjusted only in the digital clock mode, activated with the select button, and adjusted using combination of the select and rotary encoder. 

#### Todo/Addition Ideas:
- Include an edit mode for the analog clock mode aswell
- Allow ability to swap from 12 hour and 24 hour time modes

### App 3: Settings
This is the third app of the watch, selecting this app loads in the brightness adjustment page, and the user can also use the encoder to switch to an easter egg. Additional configurable settings have not yet been added. Selecting the brightness adjustment page loads in a circular bar representing the brightness along with the percentage in the center. The user can adjust the brightness using the rotary encoder, and will adjust brightness in real time. Selecting the easter egg page loads in an easter egg image from Good Bye Eri by Tatsuki Fujimoto.

#### Todo/Addition Ideas:
- Include additional Settings pages
- Include additional Settings

## Hardware Overview:

The hardware decisions were made to prioritize minimizing the size, and user experience. The display and board were chosen first, as the powerful chip and high quality 466x466 OLED display would be able to output and show nice
looking graphics and true black. The inputs were decided based on the recalibrated omnitrix from Ben 10, which features what appears to be 2 buttons and a rotating bezel, this would later prove to provide some software design challenges. The PCB was designed with the housing somewhat in mind, to ensure it would fit inside, but also would physically be possible to use all the inputs, with space for certain internal mechanisms such as the tripple bevel gear system. The housing was designed, aiming to adhere to some general 3d print and structural advice for minimum wall thickness, while also trying to minimize the size of the housing.

### BOM (Subject to change):
- 1x, Display + Board: waveshare ESP32-S3-Touch-AMOLED-1.43
  
  Purchase at: https://www.amazon.com/Waveshare-Development-Interface-Accelerometer-Gyroscope/dp/B0DT9PSDW6/ref=sr_1_2
  
  Information/Reference: https://www.waveshare.com/wiki/ESP32-S3-Touch-AMOLED-1.43.

  Notes: The Waveshare OLED esp32-s3 board was chosen in part pimarily due to the processing power and psram of the s3 and due to the quality, size, and resolution of the OLED display and the additional features included in the devboard.
  Waveshare additionally includes arduiono and esp-idf examples in C or C++ for reference. This can come with either a CO5300 Display Driver or SH8601 Display Driver, the SH8601 has not been implemented by this software, though I'm 90%    sure most are the CO5300 given that all other similar 1.43 in OLED's with different microcontrollers from Waveshare use the CO5300.

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

### PCB Schematic (V0):
  
<img width="890" height="704" alt="image" src="https://github.com/user-attachments/assets/92232321-a05d-4db2-b031-d550acabcd80" />
  
### PCB (V0):
  
<img width="558" height="445" alt="image" src="https://github.com/user-attachments/assets/6c103cb8-8472-4610-b414-a6963d79f5b3" />

### Physical Housing (incomplete):

#### Section View:
  
<img width="534" height="329" alt="image" src="https://github.com/user-attachments/assets/30da1533-3aac-428e-8af2-7b9b4dcba1ae" />

#### Full View (isometric):

<img width="544" height="509" alt="image" src="https://github.com/user-attachments/assets/27e37163-5b85-4623-951b-d487718098e8" />


### Todo/Addition Ideas:
  - Testing of core housing (3d print completed on attempt 4)
    
  <img width="1503" height="1237" alt="image" src="https://github.com/user-attachments/assets/d8be9f58-90f6-4ef6-87dc-928cb25f309d" />
 
  - Modeling and testing of base+wrist mount housing
  - Addition of additional features beyond simple 3 inputs
  - Spring Locking mechanism of the watch
  - Speaker addition

## Software Overview:

The software decisions were made under one main philosophy, step one make it work, step two make it look nice. This process resulted in a lot of initial code being quite messy, but functional and cleaned up later. Certain parts such as the IMU and RTC inits, and the UI state machine have only really reached the make it work portion, while other parts have had some attempted clean up. 

When programming an esp32 in bare metal Rust, something clear is that Rust is still a newer language, and as a result many esp32 features and support libraries, such as drivers, are incomplete or do not exist.

The cargo.toml describes all the external libaries/crates used, while the lib.rs describes the internal files used.

Included precompiled binary at esp32s3_tests.bin, can be flashed using esptools. 

To compile, set up first Rust on esp32:

https://docs.espressif.com/projects/rust/book/preface.html

The current software is divided into a few parts, the drivers and display setup, the ui state machine, the input handling and wiring, main, and the additional non Rust code helper stuff.

### The Drivers

The following drivers were implemented in Rust, due to being unable to find existing libaries/crates
#### CO5300 Display Driver

Description: 
  
This software handles the communication from the esp32-s3 to the 1.43in OLED display itself. The examples written in C from Waveshare such as ESP32-S3-AMOLED-1.43-Demo\ESP-IDF\09_FactoryProgram and the included libaries were one of     the main references for initialization and setup for the QSPI. The address and init tables were confirmed with the datasheet, but the main thing these examples allowed was for testing and confirmation that the hardware was functional.  This driver is much more complicated then the RTC and IMU drivers though, as on top of the initialization and set up, for the drivers performance and ability to update the display fast matters, not just to work in general. The driver is set up to work with the Rust Embedded Graphics Library/Crate, through draw_iter, fill_contiguous, and clear implementations, aswell as having speciic additional functions to allow speeding up certain graphics. 

While the CO5300 controller has internal GRAM for an internal frame buffer keeping the display image held, this is not readable, and due to a limitation in the writes to the display needing to be even (min 2x pixels), a software     framebuffer exists on the PSRAM. This software framebuffer is a simple box that just holds information of every pixels state on the display, allowing for odd writes to the display using the framebuffer to fill in information to make the write even without overwriting any pixels. As far as I am aware, the software framebuffer is too large to exist on the internal ram of 512 mB, due to not all that ram being available and the size of the framebuffer (466x466x2 for RGB 565), thus it lives on the PSRAM, which is unfortunately slower. This is the choice and sacrifice made in order to gain full total control of every pixel.

The driver includes a constructor with the init table to reset the display and initialize all the defaults, and a second convenient constructor to make it quicker by assuming more of the defaults. For the most part the implementation of the initialization and setup for qspi was gained from referencing the C example from waveshare. There is additionally a cmd method for sending direct cmds to the controller, and qspi_send_mode_instr which is for sending the instructions in qspi (not always used though). There are also set_window methods for both spi and qspi, setting the writeable aera of the display. Additionally the driver implments methods to control brightness and power on and off the display. 

There are three important functions to the drawing performance of the driver, the main one is the flush_fb_rect_even method that is key to the embedded graphics implementation, it pushes a rectanglular region from the framebuffer to the panel (aligned even as required by the CO5300), allowing only the dirty part to be updated. The second is the fill_rect_solid_opt, which draws a solid color rectangle to the display then optionally can update the framebuffer (used primarily for a very fast clearing of the display). The third main function is the blit_rect_be_fast_opt, this draws an image to the display through streaming a rectangle of bytes to the panel and optionally updates the frame buffer.

The driver implementation is capable of streaming a full 466x466 image at near 35 to 40 fps based on my own testing, though this is done through having the images in ram already, ready to go. This is enough performance to perform simple animations and have a reactive ui.

This is then used for display initialization handled by display.rs, setting up stuff like the frequency etc.

Location: src/co5300.rs
  
Information/Reference: https://admin.osptek.com/uploads/CO_5300_Datasheet_V0_00_20230328_07edb82936.pdf and Waveshare Example
  
#### PCF85063 RTC Driver
  Description:

  This software handles the communication from the esp32-s3 to the rtc chip, the performance of the RTC is based on how well it keeps time and not really something controlled by the software, thus the implementation of this driver was     much simpler. The implenetation of this driver is a pretty direct conversion of the C driver from waveshare, into Rust.

  Location: src/rtc_pcf85063.rs
  
  Information/Reference: https://files.waveshare.com/wiki/common/Pcf85063atl1118-NdPQpTGE-loeW7GbZ7.pdf and Waveshare Example
#### QMI8658 IMU Driver
  Description:

  This software handles communication from the esp32-s3 to the imu, this driver is very barebones however, and mainly only implements enough to function for the specific use case it has in this watch. It configures the imu, and reads
  the raw sensor data and data conversion, and built in methods for detecting the only action currently using the IMU. For further/later use, this should definitely be updated.

  Location: src/qmi8658_imu.rs
  
  Information/Reference: https://files.waveshare.com/wiki/common/QMI8658C_datasheet_rev_0.9.pdf and Waveshare Example

#### Display setup
  Description:

  Abstracts the display setup to be as simple as setup_display(), to make it easy to swap displays later that may use a different driver.

  Location: src/display.rs

  Information/Reference: 
### UI State Machine

  Description:

  This is the main state machine that handles the UI of the entire watch, it was designed with the intent to only support three button inputs (back, select, and the imu which replaces button 3) and a rotary encoder. The states were
  designed to be based on layers of menus, and the current implementation is definitely still a WIP. The UI.rs file also includes methods for animations, caching images, brightness and time adjustment, and etc it is currently doing too 
  much and definitely needs to be cleaned up. A simple stack with pop and push is used to track the state for purposes of the back button functionality. The state machine currently is definitely a bit overcomplicated and the file    
  includes nearly anything relating to the UI is in this file. 

  In order to get the snappy feel for the UI, certain graphics are preloaded images, and clears and framebuffer updates are strategically used to speed up graphics draws through knowing what the possible next states are.

  Location: src/ui.rs

  Information/Reference: 


### Input and Wiring

  Description:

  These files handle the connection to the hardware. The wiring.rs file connects and configures all the gpio pins, allowing for easy changing of gpios. There are some profiles set up, the amoled profile is the one being used, others are 
  not yet implemented. Notably, button 3 has a gpio, but in practice is not actually wired to anything as the IMU replaces it, this is related to the prototyping. The input.rs use interrupts to set flags, which are then polled for in 
  main. 

  Location: src/input.rs and src/wiring.rs

  Information/Reference: https://www.waveshare.com/wiki/ESP32-S3-Touch-AMOLED-1.43 and http://wiki.fluidnc.com/en/hardware/ESP32-S3_Pin_Reference

### Main
  Description:
  This is the main file. The main file handles setting up the interrupt handler, the display, psram, imu, etc. Before entering the main loop, the homepage is drawn on the display, then all the graphics are pre loaded. The main loop 
  handles the polling of inputs, and what to actually do when the interrupt flags are set and inputs are used, and includes logic to be able to enable a deep sleep for the watch. The loop continuously called to update the ui, but only 
  redraws when the redraw flag is set true. This makes it so most actions are only drawing once, but animations and others can set the redraw flag to true to be able to continually redraw. Much of the stuff going on, needs to be better 
  abstracted.

  Location: src/bin/main.rs

  Information/Reference: https://documentation.espressif.com/esp32-s3_datasheet_en.pdf

### Additional Helpers
  Description: There exists a lot of additional helpers such as some quick python scripts for converting image files into bytes and compressing them.

  Location: raw_images and src/assets and other files

  Information/Reference: 
  
### Todo/Addition Ideas:
  - Additional Apps
  - Clean up the ui.rs file a lot
  - Clean up main.rs
  - Make the IMU driver less specific
  - Automated Software Tests
  - A lot not mentioned 

## Testing Overview:
  All Testing was done on a breadboard prototype using throughhole components, standard switches from a random arduino kit, and the PES12-40S-N0024.

  Breadboard Prototype:
  
  <img width="1344" height="1008" alt="image" src="https://github.com/user-attachments/assets/a5cab035-a81f-4887-8b7c-62a1b8191e3c" />

  This breadboard portotype implements the same controls as the current pcb design uses, except with an addition of a third button for easier testing of the feature the imu implements. Since switches are for the most part the same, not 
  using the exact switches did not matter much, but the same rotary encoder was used to get a feel for the torque and to be testing with the right amount of detents.

  Initial Testing was done with am esp32-s3 dev kit, not this display + board. This was to test the functionality of button inputs, using simple LED outputs to confirm their function, and function of the encoder through serial print 
  statements. Testing of the inputs was for the most part insignificant though due to them being pretty standard inputs and implementations.

  Was going to use a GC9A01 LCB, but it looked bad after flashing an example program onto it, so switched to a 1.43 in OLED. 
  
  With the display + board commbo, the initial test ran was to run the example program and determine the specific driver of the display, as the driver was mentioned on the wiki to either be a CO5300 or SH8601. After running the arduino 
  example and determining the driver to be a CO5300 driver, the development of the driver software began.

  The main testing process was to iteratively build on the driver, and seeing how the display reacted in response. The testing began with the simple implementation of a full color push to the display. After some initial failures, due to an inccorectly made delay function, the display was showing a solid color. For these tests the driver initially was made with single wire spi, due to being simpler to implement, qspi later.

  <img width="1320" height="1312" alt="image" src="https://github.com/user-attachments/assets/1d36a154-8b16-4cc1-82ea-5633e61af58b" />

  After, the next test was to check if odd writes could work or not, as many displays such as the CO5300, do not properly accept odd writes. Testing was done with some simple lines and 1x1 pixel writes, which initially failed, but the 
  2x thick writes worked, showing that the display did not accept odd writes. 

  Based on this, the decision was made to use a software framebuffer on the psram, to enable odd writes, and get true full control over the pixels. A set of simple graphics tests was made to run in garbage.txt. These were set up to do a few things, fill color the display, draw text on the edges, load an image, and draw a 1 pixel thick shape and timings. After a lot of iterations and connecting the driver to the embedded graphics library/crate, the test image was showing up properly.

  <img width="1008" height="937" alt="image" src="https://github.com/user-attachments/assets/045fcda5-180c-405b-85ba-517d7c1ea20e" />

  Unfortunately, single wire was quite slow, thus the driver was refactored to support qspi (the wiring had already been set up, based on the waveshare wiki guide). This process involved interating, and continually reflashing the 
  display + board until the image showed up again properly, and the timing was much faster.

  Once the QSPI had showed up properly, a display demo was made to simple load the main graphics in a preset sequence of the UI, and time it, measuring performance in the serial monitor showed good results.

  Testing the rest of the watch was pretty simple process, of reflashing the esp32-s3, with a new updated UI, just testing that buttons and features worked as expected.

  The RTC and IMU were quite simple, and for most part worked almost first try after a bit of iterative testing to see if they worked as expected, through reading the serial monitor, and visually seeing the UI update.

  Once the current UI had been completed, it was flashed, and ran through ensuring no crashed, and full functionality.

  The same software was then uploaded onto the watch + board + input pcb combo, and proceeded to work the same after the same run through manual test. Confirmed PCB connections with a multimeter, the PCB worked despite my questionable 
  soldering.

  The watch was left overnight in low power mode and then woken up to confirm A, the battery was useable for more than a day in low power, and B to confirm the time stayed accurate with use of the RTC, succeeded.

  
  






  
