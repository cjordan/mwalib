#!/usr/bin/env python

# Given gpubox files, add their entire contents and report the sum.

# Adapted from:
# http://jakegoulding.com/rust-ffi-omnibus/objects/

# Additional documentation:
# https://docs.python.org/3.8/library/ctypes.html#module-ctypes

import sys
import argparse
import ctypes as ct
import array
import numpy as np
import numpy.ctypeslib as npct

ERROR_MESSAGE_LEN = 1024

class MwalibContextS(ct.Structure):
    pass


class MwalibMetadata(ct.Structure):
    _fields_ = [('obsid', ct.c_uint32),
                ('corr_version', ct.c_uint32),
                ('coax_v_factor', ct.c_double),
                ('start_unix_time_milliseconds', ct.c_uint64),
                ('end_unix_time_milliseconds', ct.c_uint64),
                ('duration_milliseconds', ct.c_uint64),
                ('num_timesteps', ct.c_size_t),
                ('num_antennas', ct.c_size_t),
                ('num_baselines', ct.c_size_t),
                ('num_rf_inputs', ct.c_size_t),
                ('num_antenna_pols', ct.c_size_t),
                ('num_visibility_pols', ct.c_size_t),
                ('num_coarse_channels', ct.c_size_t),
                ('integration_time_milliseconds', ct.c_uint64),
                ('fine_channel_width_hz', ct.c_uint32),
                ('observation_bandwidth_hz', ct.c_uint32),
                ('coarse_channel_width_hz', ct.c_uint32),
                ('num_fine_channels_per_coarse', ct.c_size_t),
                ('timestep_coarse_channel_bytes', ct.c_size_t),
                ('timestep_coarse_channel_floats', ct.c_size_t),
                ('num_gpubox_files', ct.c_size_t)
                ]


prefix = {"win32": ""}.get(sys.platform, "lib")
extension = {"darwin": ".dylib", "win32": ".dll"}.get(sys.platform, ".so")
path_to_mwalib = "../target/release/" + prefix + "mwalib" + extension
mwalib = ct.cdll.LoadLibrary(path_to_mwalib)

mwalib.mwalibContext_get.argtypes = \
    (ct.c_char_p,              # metafits
     ct.POINTER(ct.c_char_p),  # gpuboxes
     ct.c_size_t,              # gpubox count
     ct.c_char_p,              # error message
     ct.c_size_t)              # length of error message
mwalib.mwalibContext_get.restype = ct.POINTER(MwalibContextS)

mwalib.mwalibContext_free.argtypes = (ct.POINTER(MwalibContextS),)

mwalib.mwalibContext_read_by_baseline.argtypes = \
    (ct.POINTER(MwalibContextS), # context
     ct.c_size_t,                # input timestep_index
     ct.c_size_t,                # input coarse_channel_index
     ct.POINTER(ct.c_float),     # buffer_ptr
     ct.c_size_t)                # buffer_len
mwalib.mwalibContext_read_by_baseline.restype = ct.c_int32

mwalib.mwalibContext_read_by_frequency.argtypes = \
    (ct.POINTER(MwalibContextS), # context
     ct.c_size_t,                # input timestep_index
     ct.c_size_t,                # input coarse_channel_index
     ct.POINTER(ct.c_float),     # buffer_ptr
     ct.c_size_t)                # buffer_len
mwalib.mwalibContext_read_by_frequency.restype = ct.c_int32

mwalib.mwalibMetadata_get.argtypes = \
    (ct.POINTER(MwalibContextS),  # context_ptr
     ct.c_char_p,                 # error message
     ct.c_size_t)                 # length of error message
mwalib.mwalibMetadata_get.restype = ct.c_void_p

mwalib.mwalibMetadata_free.argtypes = (ct.c_void_p,)


class MWAlibContext:
    def __init__(self, metafits, gpuboxes):
        # Encode all inputs as UTF-8.
        m = ct.c_char_p(metafits.encode("utf-8"))

        # https://stackoverflow.com/questions/4145775/how-do-i-convert-a-python-list-into-a-c-array-by-using-ctypes
        encoded = []
        for g in gpuboxes:
            encoded.append(ct.c_char_p(g.encode("utf-8")))
        seq = ct.c_char_p * len(encoded)
        g = seq(*encoded)
        error_message = " ".encode("utf-8") * ERROR_MESSAGE_LEN
        self.context = mwalib.mwalibContext_get(m, g, len(encoded), error_message, ERROR_MESSAGE_LEN)

        if not self.context:
            print(f"Error creating context: {error_message.decode('utf-8').rstrip()}")
            exit(-1)

        # Now populate the metadata
        self.metadata = MwalibMetadata.from_address(mwalib.mwalibMetadata_get(self.context,
                                                                              error_message,
                                                                              ERROR_MESSAGE_LEN))

        if not self.metadata:
            print(f"Error creating metadata object: {error_message.decode('utf-8').rstrip()}")
            exit(-1)

        self.num_timesteps = self.metadata.num_timesteps
        self.num_floats = self.metadata.timestep_coarse_channel_floats

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        mwalib.mwalibContext_free(self.context)

    def read_by_baseline(self, timestep_index, coarse_channel_index):
        error_message = " ".encode("utf-8") * ERROR_MESSAGE_LEN

        float_buffer_type = ct.c_float * self.num_floats
        buffer = float_buffer_type()

        if mwalib.mwalibContext_read_by_baseline(self.context, ct.c_size_t(timestep_index),
                                                 ct.c_size_t(coarse_channel_index),
                                                 buffer, self.num_floats,
                                                 error_message, ERROR_MESSAGE_LEN) != 0:
            raise Exception(f"Error reading data: {error_message.decode('utf-8').rstrip()}")
        else:
            return npct.as_array(buffer, shape=(num_floats,))

    def read_by_frequency(self, timestep_index, coarse_channel_index):
        error_message = " ".encode("utf-8") * ERROR_MESSAGE_LEN

        float_buffer_type = ct.c_float * self.num_floats
        buffer = float_buffer_type()

        if mwalib.mwalibContext_read_by_baseline(self.context, ct.c_size_t(timestep_index),
                                                 ct.c_size_t(coarse_channel_index),
                                                 buffer, self.num_floats,
                                                 error_message, ERROR_MESSAGE_LEN) != 0:
            raise Exception(f"Error reading data: {error_message.decode('utf-8').rstrip()}")
        else:
            return npct.as_array(buffer, shape=(num_floats,))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("-b", "--sum-by-bl", help="Sum by baseline.", action="store_true")
    parser.add_argument("-f", "--sum-by-freq", help = "Sum by freq.", action="store_true")
    parser.add_argument("-m", "--metafits", required=True,
                        help="Path to the metafits file.")
    parser.add_argument("gpuboxes", nargs='*',
                        help="Paths to the gpubox files.")
    args = parser.parse_args()

    with MWAlibContext(args.metafits, args.gpuboxes) as context:
        num_timesteps = context.num_timesteps
        num_coarse_channels = 1
        num_fine_channels = 128
        num_baselines = 8256
        num_vis_pols = 4
        num_floats = num_baselines * \
            num_fine_channels * num_vis_pols * 2

        sum = 0.0

        if args.sum_by_bl:
            print("Summing by baseline...")
            for timestep_index in range(0, num_timesteps):
                this_sum = 0

                for coarse_channel_index in range(0, num_coarse_channels):
                    try:
                        data = context.read_by_baseline(timestep_index,
                                                        coarse_channel_index)
                    except Exception as e:
                        print(f"Error: {e}")
                        exit(-1)

                    # "data" is just an array of pointers at the moment. If one
                    # wanted to create and populate a numpy array with the raw MWA data,
                    # then the following would work.
                    # np_data = np.empty(gpubox_hdu_size),
                    #                    dtype=np.float32)
                    # for s in range(num_scans.value):
                    #     for g in range(num_gpubox_files.value):
                    #         np_data[s][g] = npct.as_array(data[s][g], shape=(gpubox_hdu_size.value,))

                    # But, in this example, we're only interested in adding all the data
                    # into a single number.
                    this_sum = np.sum(data,
                                      dtype=np.float64)

                    sum += this_sum
            print("Total sum: {}".format(sum))

        if args.sum_by_freq:
            print("Summing by frequency...")

            for timestep_index in range(0, num_timesteps):
                this_sum = 0

                for coarse_channel_index in range(0, num_coarse_channels):
                    try:
                        data = context.read_by_frequency(timestep_index,
                                                         coarse_channel_index)
                    except Exception as e:
                        print(f"Error: {e}")
                        exit(-1)

                    this_sum = np.sum(data,
                                      dtype=np.float64)

                    sum += this_sum

            print("Total sum: {}".format(sum))
