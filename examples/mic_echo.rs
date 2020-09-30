//! A basic input + output stream example, copying the mic input stream to the default output stream

extern crate coreaudio;

use std::collections::VecDeque;
use std::{mem, slice};
use std::ptr::null;
use std::sync::{Arc, Mutex};

use coreaudio::audio_unit::{AudioFormat, AudioUnit, Element, Scope, StreamFormat, SampleFormat};
use coreaudio::audio_unit::audio_format::LinearPcmFlags;
use coreaudio::audio_unit::render_callback::{self, data, Data};
use coreaudio::audio_unit::sample_format::SampleFormat::{F32, I16, I32, I8};
use coreaudio::sys::*;
use std::error::Error;

type S = f32;
const SAMPLE_FORMAT: SampleFormat = SampleFormat::F32;
const BASE_FLAGS: LinearPcmFlags = LinearPcmFlags::IS_FLOAT;

fn main() -> Result<(), coreaudio::Error> {
    let mut input_audio_unit = audio_unit_from_device(default_input_device().unwrap(), true)?;
    let mut output_audio_unit = audio_unit_from_device(default_output_device().unwrap(), false)?;

    // TODO
    // - input 1/2 channels float/signed-integer, output 1/2 channels float / signed integer

    let in_stream_format = StreamFormat {
        sample_rate: 44100.0,
        sample_format: SAMPLE_FORMAT,
        flags: BASE_FLAGS | LinearPcmFlags::IS_PACKED | LinearPcmFlags::IS_NON_INTERLEAVED,
        channels_per_frame: 1,
    };
    let in_interleaved = !in_stream_format.flags.contains(LinearPcmFlags::IS_NON_INTERLEAVED);

    let out_stream_format = StreamFormat {
        sample_rate: 44100.0,
        sample_format: SAMPLE_FORMAT,
        flags: BASE_FLAGS | LinearPcmFlags::IS_PACKED | LinearPcmFlags::IS_NON_INTERLEAVED,
        channels_per_frame: 2,
    };
    let out_interleaved = !out_stream_format.flags.contains(LinearPcmFlags::IS_NON_INTERLEAVED);
    println!("input={:#?}", &in_stream_format);
    println!("output={:#?}", &out_stream_format);
    println!("input_asbd={:#?}", &in_stream_format.to_asbd());
    println!("output_asbd={:#?}", &out_stream_format.to_asbd());

    let id = kAudioUnitProperty_StreamFormat;
    let asbd = in_stream_format.to_asbd();
    input_audio_unit.set_property(id, Scope::Output, Element::Input, Some(&asbd))?;

    let asbd = out_stream_format.to_asbd();
    output_audio_unit.set_property(id, Scope::Input, Element::Output, Some(&asbd))?;

    let buffer_left = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let producer_left = buffer_left.clone();
    let consumer_left = buffer_left.clone();
    let buffer_right = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let producer_right = buffer_right.clone();
    let consumer_right = buffer_right.clone();

    // seed roughly 1 second of data to create a delay in the feedback loop for easier testing
    for buffer in vec![buffer_left, buffer_right] {
        let mut buffer = buffer.lock().unwrap();
        for i in 0..(out_stream_format.sample_rate as i32) {
            buffer.push_back(0.0);
        }
    }

    println!("set input");
    if in_interleaved {
        type Args = render_callback::Args<data::Interleaved<'static, f32>>;
        input_audio_unit.set_input_callback(move |args| {
            let Args { num_frames, mut data, .. } = args;
            let mut buffer_left = producer_left.lock().unwrap();
            let mut buffer_right = producer_right.lock().unwrap();
            let mut buffers = vec![buffer_left, buffer_right];
            for i in 0..num_frames {
                for ch in 0..data.channels {
                    let value: f32 = data.buffer[i * data.channels + ch];
                    buffers[ch].push_back(value);
                }
            }
            Ok(())
        });
    } else {
        type Args = render_callback::Args<data::NonInterleaved<f32>>;
        input_audio_unit.set_input_callback(move |args| {
            let Args { num_frames, mut data, .. } = args;
            let mut buffer_left = producer_left.lock().unwrap();
            let mut buffer_right = producer_right.lock().unwrap();
            let mut buffers = vec![buffer_left, buffer_right];
            for i in 0..num_frames {
                for (ch, channel) in data.channels_mut().enumerate() {
                    let value: f32 = channel[i];
                    buffers[ch].push_back(value);
                }
            }
            Ok(())
        });
    }
    input_audio_unit.start()?;

    println!("set render");
    if out_interleaved {
        type Args = render_callback::Args<data::Interleaved<'static, f32>>;
        output_audio_unit.set_render_callback(move |args: Args| {
            let Args { num_frames, mut data, .. } = args;

            let mut buffer_left = consumer_left.lock().unwrap();
            let mut buffer_right = consumer_right.lock().unwrap();
            let mut buffers = vec![buffer_left, buffer_right];
            // for i in 0..num_frames {
            //     for (ch, channel) in data.channels_mut().enumerate() {
            //         let sample: f32 = buffers[ch].pop_front().unwrap_or(0.0);
            //         channel[i] = sample;
            //     }
            // }
            // for i in 0..num_frames {
            //     for ch in 0..data.channels {
            //         let sample: f32 = data.buffer[i * data.channels + ch];
            //         buffers[ch].push_back(convert_to_float(value));
            //         data.buffer[i * data.channels + ch]
            //     }
            // }
            Ok(())
        });
    } else {
        type Args = render_callback::Args<data::NonInterleaved<f32>>;
        output_audio_unit.set_render_callback(move |args: Args| {
            let Args { num_frames, mut data, .. } = args;

            let mut buffer_left = consumer_left.lock().unwrap();
            let mut buffer_right = consumer_right.lock().unwrap();
            let mut buffers = vec![buffer_left, buffer_right];
            for i in 0..num_frames {
                let zero: f32 = 0.0;
                let f: f32 = *buffers[0].front().unwrap_or(&zero);
                for (ch, channel) in data.channels_mut().enumerate() {
                    let sample: f32 = buffers[ch].pop_front().unwrap_or(f);
                    channel[i] = sample;
                }
            }
            Ok(())
        });
    }
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
