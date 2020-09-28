//! A basic input + output stream example, copying the mic input stream to the default output stream

extern crate coreaudio;

use std::collections::VecDeque;
use std::mem;
use std::ptr::null;
use std::sync::{Arc, Mutex};

use coreaudio::audio_unit::{AudioFormat, AudioUnit, Element, Scope, StreamFormat};
use coreaudio::audio_unit::audio_format::LinearPcmFlags;
use coreaudio::audio_unit::render_callback::{self, data};
use coreaudio::audio_unit::sample_format::SampleFormat::F32;
use coreaudio::sys::*;

fn main() -> Result<(), coreaudio::Error> {
    let mut input_audio_unit = audio_unit_from_device(default_input_device().unwrap(), true)?;
    let mut output_audio_unit = audio_unit_from_device(default_output_device().unwrap(), false)?;
    //AudioUnit::new(IOType::HalOutput)?;

    let stream_format = StreamFormat {
        sample_rate: 44100.0,
        sample_format: F32,
        flags: LinearPcmFlags::IS_FLOAT | LinearPcmFlags::IS_PACKED | LinearPcmFlags::IS_NON_INTERLEAVED,
        channels_per_frame: 1,
    };
    // input_audio_unit.set_stream_format(stream_format, Scope::Input)?;
    let id = kAudioUnitProperty_StreamFormat;
    let asbd = stream_format.to_asbd();
    input_audio_unit.set_property(id, Scope::Output, Element::Input, Some(&asbd))?;
    output_audio_unit.set_property(id, Scope::Input, Element::Output, Some(&asbd))?;

    let input_format = input_audio_unit.input_stream_format()?;
    println!("input={:#?}", &input_format);
    println!("input={:#?}", &asbd);

    let output_format = output_audio_unit.output_stream_format()?;
    println!("output={:#?}", &output_format);

    // let id = kAudioHardwarePropertyDefaultInputDevice;
    // let mut out_size  = 0;
    // let mut out_data;
    // let status = unsafe { AudioHardwareGetProperty(id, &mut out_size, &mut out_data) };
    // audio_unit.set_property(id, Scope::Global, Element::Output, Some(&out_data))?;

    // let id = kAudioOutputUnitProperty_EnableIO;
    // let flag = 1u32;
    // let yo = audio_unit.get_property(id, Scope::Input, Element::Input)?;
    // println!("enabled={:#?}", yo);
    // audio_unit.set_property(id, Scope::Input, Element::Input, Some(&flag))?;

    // let input_device_id = default_input_device().unwrap();
    // println!("got device {}", input_device_id);
    // let id = kAudioOutputUnitProperty_CurrentDevice;
    // audio_unit.set_property(id, Scope::Global, Element::Output, Some(&input_device_id))?;

    type Args = render_callback::Args<data::NonInterleaved<f32>>;

    // let buffer_list = AudioBufferList::default();
    let buffer = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let producer = buffer.clone();
    let consumer = buffer.clone();

    println!("set input");
    // {
    //     let id = kAudioUnitProperty_StreamFormat;
    //     let asbd = input_audio_unit.get_property(id, Scope::Input, Element::Input)?;
        // println!("wut={:#?}", &asbd);
    // }
    input_audio_unit.set_input_callback(move |args| {
        // println!("hi");
        let Args { num_frames, mut data, .. } = args;
        let mut buffer = producer.lock().unwrap();
        // println!("num frames {}", num_frames);
        for i in 0..num_frames {
            // just take the 1st channel, good enough for demo purposes
            for channel in data.channels_mut() {
                // for sample in channel {
                    let value = channel[i];
                    if value.abs() > 0.1 {
                        println!("push {}", value);
                    }
                    buffer.push_back(value);
                // }
                break;
            }
            // for mic_buf in data.channels_mut().into_iter().next() {
            //     for sample in mic_buf {
            //         println!("push {}", *sample);
            //         buffer.push_back(*sample);
            //     }
            // }
        }
        // println!("mic buf sz={}", buffer.len());
        Ok(())
    })?;
    input_audio_unit.start()?;

    println!("set render");
    output_audio_unit.set_render_callback(move |args| {
        let Args { num_frames, mut data, .. } = args;
        let mut buffer = consumer.lock().unwrap();
        // println!("out buf sz={}", buffer.len());
        for i in 0..num_frames {
            for channel in data.channels_mut() {
                let sample = buffer.pop_front().unwrap_or(0.0);
                channel[i] = sample;
                if sample.abs() > 0.1 {
                    println!("yo {}", channel[i]);
                }
            }
        }
        Ok(())
    })?;
    output_audio_unit.start()?;

    std::thread::sleep(std::time::Duration::from_millis(100000));

    Ok(())
}


/// Copied from cpal
pub fn default_input_device() -> Option<AudioDeviceID> {
    let property_address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyDefaultInputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMaster,
    };

    let audio_device_id: AudioDeviceID = 0;
    let data_size = mem::size_of::<AudioDeviceID>();
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &property_address as *const _,
            0,
            null(),
            &data_size as *const _ as *mut _,
            &audio_device_id as *const _ as *mut _,
        )
    };
    if status != kAudioHardwareNoError as i32 {
        return None;
    }

    Some(audio_device_id)
}

pub fn default_output_device() -> Option<AudioDeviceID> {
    let property_address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyDefaultOutputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMaster,
    };

    let audio_device_id: AudioDeviceID = 0;
    let data_size = mem::size_of::<AudioDeviceID>();
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &property_address as *const _,
            0,
            null(),
            &data_size as *const _ as *mut _,
            &audio_device_id as *const _ as *mut _,
        )
    };
    if status != kAudioHardwareNoError as i32 {
        return None;
    }

    Some(audio_device_id)
}

fn audio_unit_from_device(device_id: AudioDeviceID, input: bool) -> Result<AudioUnit, coreaudio::Error> {
    let mut audio_unit = {
        let au_type = if cfg!(target_os = "ios") {
            // The HalOutput unit isn't available in iOS unfortunately.
            // RemoteIO is a sensible replacement.
            // See https://goo.gl/CWwRTx
            coreaudio::audio_unit::IOType::RemoteIO
        } else {
            coreaudio::audio_unit::IOType::HalOutput
        };
        AudioUnit::new(au_type)?
    };

    if input {
        // Enable input processing.
        let enable_input = 1u32;
        audio_unit.set_property(
            kAudioOutputUnitProperty_EnableIO,
            Scope::Input,
            Element::Input,
            Some(&enable_input),
        )?;

        // Disable output processing.
        let disable_output = 0u32;
        audio_unit.set_property(
            kAudioOutputUnitProperty_EnableIO,
            Scope::Output,
            Element::Output,
            Some(&disable_output),
        )?;
    }

    audio_unit.set_property(
        kAudioOutputUnitProperty_CurrentDevice,
        Scope::Global,
        Element::Output,
        Some(&device_id),
    )?;

    Ok(audio_unit)
}
