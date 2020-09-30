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

fn sample_format_from_str(s: &str) -> Option<SampleFormat> {
    match s.to_ascii_lowercase().as_str() {
        "f32" => Some(F32),
        "i8" => Some(I8),
        "i16" => Some(I16),
        "i32" => Some(I32),
        _ => None,
    }
}

fn interleaved_from_str(s: &str) -> Option<bool> {
    match s.to_ascii_lowercase().as_str() {
        "interleaved" => Some(true),
        "noninterleaved" => Some(false),
        _ => None,
    }
}

fn compute_flags(sample_format: SampleFormat, interleaved: bool) -> LinearPcmFlags {
    let mut out = LinearPcmFlags::IS_PACKED;
    out |= match sample_format {
        F32 => LinearPcmFlags::IS_FLOAT,
        I32 | I16 | I8 => LinearPcmFlags::IS_SIGNED_INTEGER,
    };
    if !interleaved {
        out |= LinearPcmFlags::IS_NON_INTERLEAVED
    }
    out
}

fn main() -> Result<(), coreaudio::Error> {
    let mut input_audio_unit = audio_unit_from_device(default_input_device().unwrap(), true)?;
    let mut output_audio_unit = audio_unit_from_device(default_output_device().unwrap(), false)?;
    //AudioUnit::new(IOType::HalOutput)?;

    // let default_sample_rates: Vec<f32> = vec![44100.0, 96000.0];
    // let sample_formats = vec![F32, I16];
    // let channels: Vec<u32> = vec![1, 2];

    let mut in_rate = 44100.0 as f64;
    let mut in_format = F32;
    let mut in_channels = 1;
    let mut in_interleaved: bool = true;

    let mut out_rate = 44100.0 as f64;
    let mut out_format = F32;
    let mut out_channels = 1;
    let mut out_interleaved: bool = true;

    for (index, arg) in std::env::args().enumerate() {
        match index {
            0 => in_rate = arg.parse::<f64>().unwrap(),
            1 => in_format = sample_format_from_str(arg.as_str()).unwrap(),
            2 => in_channels = arg.parse::<u32>().unwrap(),
            3 => in_interleaved = interleaved_from_str(arg.as_str()).unwrap(),
            4 => out_rate = arg.parse::<f64>().unwrap(),
            5 => out_format = sample_format_from_str(arg.as_str()).unwrap(),
            6 => out_channels = arg.parse::<u32>().unwrap(),
            7 => out_interleaved = interleaved_from_str(arg.as_str()).unwrap(),
            _ => {}
        }
    }

    // TODO
    // - input 1/2 channels float/signed-integer, output 1/2 channels float / signed integer

    let in_stream_format = StreamFormat {
        sample_rate: in_rate,
        sample_format: in_format,
        flags: compute_flags(in_format, in_interleaved),
        channels_per_frame: in_channels,
    };

    let out_stream_format = StreamFormat {
        sample_rate: out_rate,
        sample_format: out_format,
        flags: compute_flags(out_format, out_interleaved),
        channels_per_frame: out_channels,
    };
    println!("input={:#?}", &in_stream_format);
    println!("output={:#?}", &out_stream_format);
    println!("input_asbd={:#?}", &in_stream_format.to_asbd());
    println!("output_asbd={:#?}", &out_stream_format.to_asbd());

    let id = kAudioUnitProperty_StreamFormat;
    let asbd = in_stream_format.to_asbd();
    input_audio_unit.set_property(id, Scope::Output, Element::Input, Some(&asbd))?;

    let asbd = out_stream_format.to_asbd();
    output_audio_unit.set_property(id, Scope::Input, Element::Output, Some(&asbd))?;


    // let buffer_list = AudioBufferList::default();
    let buffer_left = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let producer_left = buffer_left.clone();
    let consumer_left = buffer_left.clone();
    let buffer_right = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let producer_right = buffer_right.clone();
    let consumer_right = buffer_right.clone();

    // seed roughly 1 second of data to create a delay in the feedback loop for easier testing
    for buffer in vec![buffer_left, buffer_right] {
        let mut buffer = buffer.lock().unwrap();
        for i in 0..(out_rate as i32) {
            buffer.push_back(0.0);
        }
    }

    println!("set input");

    match in_format {
        F32 => {
            setup_input_callback(&mut input_audio_unit, in_interleaved, producer_left, producer_right,
                                 |s: f32| s);
        }
        I32 => {
            setup_input_callback(&mut input_audio_unit, in_interleaved, producer_left, producer_right,
                                 |s: i32| s as f32 / i32::max_value() as f32);
        }
        I16 => {
            setup_input_callback(&mut input_audio_unit, in_interleaved, producer_left, producer_right,
                                 |s: i16| s as f32 / i16::max_value() as f32);
        }
        I8 => {
            setup_input_callback(&mut input_audio_unit, in_interleaved, producer_left, producer_right,
                                 |s: i8| s as f32 / i8::max_value() as f32);
        }
    }
    input_audio_unit.start()?;

    println!("set render");
    // setup_render_callback(output_audio_unit, out_format, out_interleaved, consumer_left, consumer_right);

    fn render_data_callback<S, C>(data_list: Vec<Data<S>>, convert_from_float: C) {
        let mut buffer_left = buffer_left.lock().unwrap();
        let mut buffer_right = buffer_right.lock().unwrap();
        let mut buffers = vec![buffer_left, buffer_right];
        for i in 0..num_frames {
            if interleaved {
                // for (ch, channel) in data.channels_mut().enumerate() {
                //     let sample: f32 = buffers[ch].pop_front().unwrap_or(0.0);
                //     channel[i] = convert_from_float(sample);
                // }

            } else {
                for (ch, channel) in data.channels_mut().enumerate() {
                    let sample: f32 = buffers[ch].pop_front().unwrap_or(0.0);
                    channel[i] = convert_from_float(sample);
                }
            }
        }
    }

    match out_format {
        F32 => {
            setup_render_callback(&mut output_audio_unit, out_format, |data_list: Vec<Data<f32>>| {

            });
        }
        I32 => {
            setup_render_callback(&mut output_audio_unit, out_interleaved, consumer_left, consumer_right,
                                 |s: f32| (s * i32::max_value() as f32).round());
        }
        I16 => {
            setup_render_callback(&mut output_audio_unit, out_interleaved, consumer_left, consumer_right,
                                  |s: f32| (s * i16::max_value() as f32).round());
        }
        I8 => {
            setup_render_callback(&mut output_audio_unit, out_interleaved, consumer_left, consumer_right,
                                  |s: f32| (s * i8::max_value() as f32).round());
        }
    }
    output_audio_unit.start()?;

    std::thread::sleep(std::time::Duration::from_millis(100000));

    Ok(())
}


fn setup_input_callback<C, S>(
    audio_unit: &mut AudioUnit,
    interleaved: bool,
    buffer_left: Arc<Mutex<VecDeque<f32>>>,
    buffer_right: Arc<Mutex<VecDeque<f32>>>,
    convert_to_float: C,
) -> Result<(), coreaudio::Error>
    where C: Fn(S) -> f32 + Send + 'static,
{
    if interleaved {
        Err(coreaudio::Error::Unspecified)
    } else {
        type Args = render_callback::Args<data::NonInterleaved<S>>;
        audio_unit.set_input_callback(move |args| {
            let Args { num_frames, mut data, .. } = args;
            let mut buffer_left = buffer_left.lock().unwrap();
            let mut buffer_right = buffer_right.lock().unwrap();
            let mut buffers = vec![buffer_left, buffer_right];
            for i in 0..num_frames {
                for (ch, channel) in data.channels_mut().enumerate() {
                    let value: S = channel[i];
                    buffers[ch].push_back(convert_to_float(value));
                }
            }
            Ok(())
        })
    }
}

fn setup_render_callback(audio_unit: &mut AudioUnit, data_callback: D) -> Result<(), coreaudio::Error>
where D: Fn(S) -> f32 + Send + 'static,
{
type Args = render_callback::Args<data::Raw>;
    audio_unit.set_render_callback(move |args: Args| {
        let Args { num_frames, mut data, .. } = args;

        let ptr = (*data).mBuffers.as_ptr() as *const AudioBuffer;
        let len = (*data).mNumberBuffers as usize;
        let buffers: &[AudioBuffer] = unsafe { slice::from_raw_parts(ptr, len) };

        let mut data_list: Vec<&[()]> = vec![];
        for buffer in buffers {
            let AudioBuffer {
                mDataByteSize: data_byte_size,
                mData: data,
            } = buffer;

            let data = data as *mut ();
            let len = (data_byte_size as usize / bytes_per_channel) as usize;
            data_list.push(unsafe { slice::from_raw_parts(data, len) });
        }
        data_callback(data_list);

        // let mut buffer_left = buffer_left.lock().unwrap();
        // let mut buffer_right = buffer_right.lock().unwrap();
        // let mut buffers = vec![buffer_left, buffer_right];
        // for i in 0..num_frames {
        //     if interleaved {
        //         // for (ch, channel) in data.channels_mut().enumerate() {
        //         //     let sample: f32 = buffers[ch].pop_front().unwrap_or(0.0);
        //         //     channel[i] = convert_from_float(sample);
        //         // }
        //
        //     } else {
        //         for (ch, channel) in data.channels_mut().enumerate() {
        //             let sample: f32 = buffers[ch].pop_front().unwrap_or(0.0);
        //             channel[i] = convert_from_float(sample);
        //         }
        //     }
        // }
        Ok(())
    })
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
