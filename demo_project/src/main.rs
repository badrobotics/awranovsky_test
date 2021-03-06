#![no_std]
#![no_main]

extern crate atomic_queue;

use core::fmt::Write;

use rust_tm4c::tm4c_peripherals::get_peripherals;
use rust_tm4c::gpio;
use rust_tm4c::system_control;
use rust_tm4c::uart;
use rust_tm4c::timer;
use rust_tm4c::interrupt;

use atomic_queue::AtomicQueue;

const CPU_FREQ: u32  = 120_000_000;
const XTAL_FREQ: u32 = 25_000_000;

#[no_mangle]
pub fn main() -> ! {
    let mut p = get_peripherals();
    let scb = p.take_scb().unwrap();
    let nvic = p.take_nvic().unwrap();
    let sysctl = p.take_system_control().unwrap();
    let gpion = p.take_gpion().unwrap();
    let gpioa = p.take_gpioa().unwrap();
    let mut uart0 = p.take_uart0().unwrap();
    let timer0 = p.take_timer0().unwrap();
    let timer1 = p.take_timer1().unwrap();

    // Configure the CPU for the maximum operating frequency
    let cpu_freq = sysctl.tm4c129_config_sysclk(CPU_FREQ, XTAL_FREQ);

    // Set up LEDs
    sysctl.enable_gpio_clock(system_control::GpioPort::GpioN);
    gpion.configure_as_output(gpio::Pin::Pin0);
    gpion.configure_as_output(gpio::Pin::Pin1);
    unsafe { GPIO_BLOCK = Some(gpion); }

    // Set up the debug UART
    sysctl.enable_gpio_clock(system_control::GpioPort::GpioA);
    sysctl.enable_uart_clock(system_control::Uart::Uart0);
    gpioa.select_alternate_function(gpio::Pin::Pin0, 1);
    gpioa.select_alternate_function(gpio::Pin::Pin1, 1);
    let _baud = uart0
        .configure(
            CPU_FREQ,
            115200,
            uart::Parity::None,
            uart::StopBits::One,
            uart::WordLength::Eight,
        )
        .unwrap();
    let mut uart_driver = uart::drivers::UartBlockingDriver::new(&mut uart0, uart::drivers::NewlineMode::CRLF);

    // Set up timers to trigger at slightly different frequencies
    sysctl.enable_timer_clock(system_control::Timer::Timer0);
    sysctl.enable_timer_clock(system_control::Timer::Timer1);
    match timer0.set_periodic_mode_32bit(10000) { _ => {} }; // Timer 1 should trigger first, since it's a lower priority
    match timer1.set_periodic_mode_32bit(10001) { _ => {} };

    // Set up interrupts
    scb.int_register(interrupt::IntType::Timer0A, timer0a_handler);
    scb.int_register(interrupt::IntType::Timer1A, timer1a_handler);
    nvic.clear_pending(interrupt::IntType::Timer0A);
    nvic.clear_pending(interrupt::IntType::Timer1A);
    nvic.set_priority(interrupt::IntType::Timer0A, 1); // Make timer 0 a lower priority so it can be preempted
    nvic.set_priority(interrupt::IntType::Timer1A, 0);
    nvic.enable(interrupt::IntType::Timer0A);
    nvic.enable(interrupt::IntType::Timer1A);

    // Create the queue
    let mut storage: [u8; 16] = [0; 16];
    let ref queue: AtomicQueue<u8> = {
        let m = AtomicQueue::new(&mut storage);
        m
    };

    // Fill the first two slots in the queue with dummy variables
    match queue.push(0) {
        Err(_) => panic!("No room to push?"),
        Ok(_) => {},
    }
    match queue.push(0) {
        Err(_) => panic!("No room to push?"),
        Ok(_) => {},
    }

    // Give the timer interrupts access to the timers
    unsafe { TIMER0 = Some(timer0); }
    unsafe { TIMER1 = Some(timer1); }

    let mut counter = 0_u8;
    loop {
        writeln!(uart_driver, "Hello, world! counter={} two_values_ago={}", counter, queue.pop().unwrap()).unwrap();
        match queue.push(counter) {
            Err(_) => panic!("No room to push?"),
            Ok(_) => {},
        }
        counter = counter.wrapping_add(1);
    }
}

static mut TIMER0: Option<&'static mut timer::Timer> = None;
pub unsafe extern "C" fn timer0a_handler() {
    if let Some(t) = &mut TIMER_BLOCK {
        match t.clear_timeout_interrupt_32bit() {
            _ => {},
        }
    }
}

static mut TIMER1: Option<&'static mut timer::Timer> = None;
pub unsafe extern "C" fn timer1a_handler() {
    if let Some(t) = &mut TIMER1 {
        match t.clear_timeout_interrupt_32bit() {
            _ => {},
        }
    }
}
