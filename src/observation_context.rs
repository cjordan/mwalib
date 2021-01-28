// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/*!
The main interface to MWA data.
 */
use std::fmt;

use chrono::{DateTime, Duration, FixedOffset};

use crate::antenna::*;
use crate::coarse_channel::*;
use crate::rfinput::*;
use crate::*;

/// `mwalib` observation context. This represents the basic metadata for the observation.
///
pub struct ObservationContext {
    /// Observation id
    pub obsid: u32,
    /// Latitude of centre point of MWA in raidans
    pub mwa_latitude_radians: f64,
    /// Longitude of centre point of MWA in raidans
    pub mwa_longitude_radians: f64,
    /// Altitude of centre poing of MWA in metres
    pub mwa_altitude_metres: f64,
    /// the velocity factor of electic fields in RG-6 like coax
    pub coax_v_factor: f64,
    /// Scheduled start (gps time) of observation
    pub scheduled_start_gpstime_milliseconds: u64,
    /// Scheduled end (gps time) of observation
    pub scheduled_end_gpstime_milliseconds: u64,
    /// Scheduled start (UNIX time) of observation
    pub scheduled_start_unix_time_milliseconds: u64,
    /// Scheduled end (UNIX time) of observation
    pub scheduled_end_unix_time_milliseconds: u64,
    /// Scheduled start (UTC) of observation
    pub scheduled_start_utc: DateTime<FixedOffset>,
    /// Scheduled end (UTC) of observation
    pub scheduled_end_utc: DateTime<FixedOffset>,
    /// Scheduled start (MJD) of observation
    pub scheduled_start_mjd: f64,
    /// Scheduled end (MJD) of observation
    pub scheduled_end_mjd: f64,
    /// Scheduled duration of observation
    pub scheduled_duration_milliseconds: u64,
    /// RA tile pointing
    pub ra_tile_pointing_degrees: f64,
    /// DEC tile pointing
    pub dec_tile_pointing_degrees: f64,
    /// RA phase centre
    pub ra_phase_center_degrees: Option<f64>,
    /// DEC phase centre
    pub dec_phase_center_degrees: Option<f64>,
    /// AZIMUTH
    pub azimuth_degrees: f64,
    /// ALTITUDE
    pub altitude_degrees: f64,
    /// Altitude of Sun
    pub sun_altitude_degrees: f64,
    /// Distance from pointing center to Sun
    pub sun_distance_degrees: f64,
    /// Distance from pointing center to the Moon
    pub moon_distance_degrees: f64,
    /// Distance from pointing center to Jupiter
    pub jupiter_distance_degrees: f64,
    /// Local Sidereal Time
    pub lst_degrees: f64,
    /// Hour Angle of pointing center (as a string)
    pub hour_angle_string: String,
    /// GRIDNAME
    pub grid_name: String,
    /// GRIDNUM
    pub grid_number: i32,
    /// CREATOR
    pub creator: String,
    /// PROJECT
    pub project_id: String,
    /// Observation name
    pub observation_name: String,
    /// MWA observation mode
    pub mode: String,
    /// RECVRS    // Array of receiver numbers (this tells us how many receivers too)
    pub receivers: Vec<usize>,
    /// DELAYS    // Array of delays
    pub delays: Vec<usize>,
    /// ATTEN_DB  // global analogue attenuation, in dB
    pub global_analogue_attenuation_db: f64,
    /// Seconds of bad data after observation starts
    pub quack_time_duration_milliseconds: u64,
    /// OBSID+QUACKTIM as Unix timestamp (first good timestep)
    pub good_time_unix_milliseconds: u64,
    /// Total number of antennas (tiles) in the array
    pub num_antennas: usize,
    /// We also have just the antennas
    pub antennas: Vec<Antenna>,
    /// Total number of rf_inputs (tiles * 2 pols X&Y)    
    pub num_rf_inputs: usize,
    /// The Metafits defines an rf chain for antennas(tiles) * pol(X,Y)
    pub rf_inputs: Vec<RFInput>,
    /// Number of antenna pols. e.g. X and Y
    pub num_antenna_pols: usize,
    /// Number of coarse channels after we've validated the input gpubox files
    pub num_coarse_channels: usize,
    /// Vector of coarse channel structs
    pub coarse_channels: Vec<CoarseChannel>,
    /// Total bandwidth of observation (of the coarse channels we have)
    pub observation_bandwidth_hz: u32,
    /// Bandwidth of each coarse channel
    pub coarse_channel_width_hz: u32,
    /// The value of the FREQCENT key in the metafits file, but in Hz.
    pub metafits_centre_freq_hz: u32,
    /// Filename of the metafits we were given
    pub metafits_filename: String,
}

impl ObservationContext {
    pub fn new<T: AsRef<std::path::Path>>(metafits: &T) -> Result<Self, MwalibError> {
        // Pull out observation details. Save the metafits HDU for faster
        // accesses.
        let mut metafits_fptr = fits_open!(&metafits)?;
        let metafits_hdu = fits_open_hdu!(&mut metafits_fptr, 0)?;
        let metafits_tile_table_hdu = fits_open_hdu!(&mut metafits_fptr, 1)?;

        // Populate obsid from the metafits
        let obsid = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "GPSTIME")?;

        // from MWA_Tools/CONV2UVFITS/convutils.h
        // Used to determine electrical lengths if EL_ not present in metafits for an rf_input
        let coax_v_factor: f64 = 1.204;
        let quack_time_duration_milliseconds: u64 = {
            let qt: f64 = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "QUACKTIM")?;
            (qt * 1000.).round() as _
        };
        let good_time_unix_milliseconds: u64 = {
            let gt: f64 = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "GOODTIME")?;
            (gt * 1000.).round() as _
        };

        // Create a vector of rf_input structs from the metafits
        let num_rf_inputs: usize =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "NINPUTS")?;

        // There are twice as many inputs as
        // there are antennas; halve that value.
        let num_antennas = num_rf_inputs / 2;

        // Create a vector of rf_input structs from the metafits
        let mut rf_inputs: Vec<RFInput> = RFInput::populate_rf_inputs(
            num_rf_inputs,
            &mut metafits_fptr,
            metafits_tile_table_hdu,
            coax_v_factor,
        )?;

        // Sort the rf_inputs back into the correct output order
        rf_inputs.sort_by_key(|k| k.subfile_order);

        // Now populate the antennas (note they need to be sorted by subfile_order)
        let antennas: Vec<Antenna> = Antenna::populate_antennas(&rf_inputs);

        // Always assume that MWA antennas have 2 pols
        let num_antenna_pols = 2;

        // The FREQCENT value in the metafits is in units of kHz - make it Hz.
        let metafits_centre_freq_hz: u32 = {
            let cf: f64 = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "FREQCENT")?;
            (cf * 1e6).round() as _
        };

        // populate lots of useful metadata
        let scheduled_start_utc_string: String =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "DATE-OBS")?;

        let scheduled_start_utc_string_with_offset: String = scheduled_start_utc_string + "+00:00";

        let scheduled_start_utc =
            DateTime::parse_from_rfc3339(&scheduled_start_utc_string_with_offset)
                .expect("Unable to parse DATE-OBS into a date time");
        let scheduled_start_mjd: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "MJD")?;
        let scheduled_duration_milliseconds: u64 = {
            let ex: u64 = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "EXPOSURE")?;
            ex * 1000
        };
        let scheduled_end_utc =
            scheduled_start_utc + Duration::milliseconds(scheduled_duration_milliseconds as i64);

        // To increment the mjd we need to fractional proportion of the day that the duration represents
        let scheduled_end_mjd =
            scheduled_start_mjd + (scheduled_duration_milliseconds as f64 / 1000. / 86400.);

        let scheduled_start_gpstime_milliseconds: u64 = obsid as u64 * 1000;
        let scheduled_end_gpstime_milliseconds: u64 =
            scheduled_start_gpstime_milliseconds + scheduled_duration_milliseconds;

        let scheduled_start_unix_time_milliseconds: u64 =
            good_time_unix_milliseconds - quack_time_duration_milliseconds;
        let scheduled_end_unix_time_milliseconds: u64 =
            scheduled_start_unix_time_milliseconds + scheduled_duration_milliseconds;

        let ra_tile_pointing_degrees: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "RA")?;
        let dec_tile_pointing_degrees: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "DEC")?;
        let ra_phase_center_degrees: Option<f64> =
            get_optional_fits_key!(&mut metafits_fptr, &metafits_hdu, "RAPHASE")?;
        let dec_phase_center_degrees: Option<f64> =
            get_optional_fits_key!(&mut metafits_fptr, &metafits_hdu, "DECPHASE")?;
        let azimuth_degrees: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "AZIMUTH")?;
        let altitude_degrees: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "ALTITUDE")?;
        let sun_altitude_degrees: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "SUN-ALT")?;
        let sun_distance_degrees: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "SUN-DIST")?;
        let moon_distance_degrees: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "MOONDIST")?;
        let jupiter_distance_degrees: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "JUP-DIST")?;
        let lst_degrees: f64 = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "LST")?;
        let hour_angle_string = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "HA")?;
        let grid_name = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "GRIDNAME")?;
        let grid_number = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "GRIDNUM")?;
        let creator = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "CREATOR")?;
        let project_id = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "PROJECT")?;
        let observation_name =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "FILENAME")?;
        let mode = get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "MODE")?;
        let receivers_string: String =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "RECVRS")?;

        let receivers: Vec<usize> = receivers_string
            .replace(&['\'', '&'][..], "")
            .split(',')
            .map(|s| s.parse().unwrap())
            .collect();

        let delays_string: String =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "DELAYS")?;

        let delays: Vec<usize> = delays_string
            .replace(&['\'', '&'][..], "")
            .split(',')
            .map(|s| s.parse().unwrap())
            .collect();

        let global_analogue_attenuation_db: f64 =
            get_required_fits_key!(&mut metafits_fptr, &metafits_hdu, "ATTEN_DB")?;

        Ok(ObservationContext {
            mwa_latitude_radians: MWA_LATITUDE_RADIANS,
            mwa_longitude_radians: MWA_LONGITUDE_RADIANS,
            mwa_altitude_metres: MWA_ALTITUDE_METRES,
            coax_v_factor,
            obsid,
            scheduled_start_gpstime_milliseconds,
            scheduled_end_gpstime_milliseconds,
            scheduled_start_unix_time_milliseconds,
            scheduled_end_unix_time_milliseconds,
            scheduled_start_utc,
            scheduled_end_utc,
            scheduled_start_mjd,
            scheduled_end_mjd,
            scheduled_duration_milliseconds,
            ra_tile_pointing_degrees,
            dec_tile_pointing_degrees,
            ra_phase_center_degrees,
            dec_phase_center_degrees,
            azimuth_degrees,
            altitude_degrees,
            sun_altitude_degrees,
            sun_distance_degrees,
            moon_distance_degrees,
            jupiter_distance_degrees,
            lst_degrees,
            hour_angle_string,
            grid_name,
            grid_number,
            creator,
            project_id,
            observation_name,
            mode,
            receivers,
            delays,
            global_analogue_attenuation_db,
            quack_time_duration_milliseconds,
            good_time_unix_milliseconds,
            num_antennas,
            antennas,
            num_rf_inputs,
            rf_inputs,
            num_antenna_pols,
            coarse_channel_width_hz,
            coarse_channels,
            num_coarse_channels,
            observation_bandwidth_hz,
            metafits_centre_freq_hz,
            metafits_filename: metafits
                .as_ref()
                .to_str()
                .expect("Metafits filename is not UTF-8 compliant")
                .to_string(),
        })
    }
}

/// Implements fmt::Display for ObservationContext struct
///
/// # Arguments
///
/// * `f` - A fmt::Formatter
///
///
/// # Returns
///
/// * `fmt::Result` - Result of this method
///
///
#[cfg(not(tarpaulin_include))]
impl fmt::Display for ObservationContext {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            r#"ObservationContext (
    MWA latitude:             {mwa_lat} degrees,
    MWA longitude:            {mwa_lon} degrees
    MWA altitude:             {mwa_alt} m,

    obsid:                    {obsid},
    mode:                     {mode},

    Creator:                  {creator},
    Project ID:               {project_id},
    Observation Name:         {obs_name},
    Receivers:                {receivers:?},
    Delays:                   {delays:?},
    Global attenuation:       {atten} dB,

    Scheduled start (UNIX)    {sched_start_unix},
    Scheduled end (UNIX)      {sched_end_unix},
    Scheduled start (GPS)     {sched_start_gps},
    Scheduled end (GPS)       {sched_end_gps},
    Scheduled start (utc)     {sched_start_utc},
    Scheduled end (utc)       {sched_end_utc},
    Scheduled start (MJD)     {sched_start_mjd},
    Scheduled end (MJD)       {sched_end_mjd},
    Scheduled duration        {sched_duration} s,
    Quack time:               {quack_duration} s,
    Good UNIX start time:     {good_time},

    R.A. (tile_pointing):     {rtpc} degrees,
    Dec. (tile_pointing):     {dtpc} degrees,
    R.A. (phase center):      {rppc},
    Dec. (phase center):      {dppc},
    Azimuth:                  {az} degrees,
    Altitude:                 {alt} degrees,
    Sun altitude:             {sun_alt} degrees,
    Sun distance:             {sun_dis} degrees,
    Moon distance:            {moon_dis} degrees,
    Jupiter distance:         {jup_dis} degrees,
    LST:                      {lst} degrees,
    Hour angle:               {ha} degrees,
    Grid name:                {grid},
    Grid number:              {grid_n},

    num antennas:             {n_ants},
    antennas:                 {ants:?},
    rf_inputs:                {rfs:?},

    num antenna pols:         {n_aps},

    metafits FREQCENT key:    {freqcent} MHz,

    metafits filename:        {meta},
)"#,
            mwa_lat = self.mwa_latitude_radians.to_degrees(),
            mwa_lon = self.mwa_longitude_radians.to_degrees(),
            mwa_alt = self.mwa_altitude_metres,
            obsid = self.obsid,
            creator = self.creator,
            project_id = self.project_id,
            obs_name = self.observation_name,
            receivers = self.receivers,
            delays = self.delays,
            atten = self.global_analogue_attenuation_db,
            sched_start_unix = self.scheduled_start_unix_time_milliseconds as f64 / 1e3,
            sched_end_unix = self.scheduled_end_unix_time_milliseconds as f64 / 1e3,
            sched_start_gps = self.scheduled_start_gpstime_milliseconds as f64 / 1e3,
            sched_end_gps = self.scheduled_end_gpstime_milliseconds as f64 / 1e3,
            sched_start_utc = self.scheduled_start_utc,
            sched_end_utc = self.scheduled_end_utc,
            sched_start_mjd = self.scheduled_start_mjd,
            sched_end_mjd = self.scheduled_end_mjd,
            sched_duration = self.scheduled_duration_milliseconds as f64 / 1e3,
            quack_duration = self.quack_time_duration_milliseconds as f64 / 1e3,
            good_time = self.good_time_unix_milliseconds as f64 / 1e3,
            rtpc = self.ra_tile_pointing_degrees,
            dtpc = self.dec_tile_pointing_degrees,
            rppc = if let Some(rppc) = self.ra_phase_center_degrees {
                format!("{} degrees", rppc)
            } else {
                "N/A".to_string()
            },
            dppc = if let Some(dppc) = self.dec_phase_center_degrees {
                format!("{} degrees", dppc)
            } else {
                "N/A".to_string()
            },
            az = self.azimuth_degrees,
            alt = self.altitude_degrees,
            sun_alt = self.sun_altitude_degrees,
            sun_dis = self.sun_distance_degrees,
            moon_dis = self.moon_distance_degrees,
            jup_dis = self.jupiter_distance_degrees,
            lst = self.lst_degrees,
            ha = self.hour_angle_string,
            grid = self.grid_name,
            grid_n = self.grid_number,
            n_ants = self.num_antennas,
            ants = self.antennas,
            rfs = self.rf_inputs,
            n_aps = self.num_antenna_pols,
            freqcent = self.metafits_centre_freq_hz as f64 / 1e6,
            mode = self.mode,
            meta = self.metafits_filename,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::*;

    #[test]
    fn test_obs_context_new_invalid_metafits() {
        let metafits_filename = "invalid.metafits";

        // No gpubox files provided
        let context = ObservationContext::new(&metafits_filename);

        assert!(context.is_err());
    }

    #[test]
    #[allow(clippy::cognitive_complexity)]
    fn test_obs_context_legacy_v1() {
        // Open the test mwa v 1 metafits file
        let metafits_filename = "test_files/1101503312_1_timestep/1101503312.metafits";

        //
        // Read the observation using mwalib
        //
        // Open a context and load in a test metafits
        let context = ObservationContext::new(&metafits_filename)
            .expect("Failed to create ObservationContext");

        // Test the properties of the context object match what we expect

        // MWA latitude:             -26.703319405555554 degrees,
        assert!(approx_eq!(
            f64,
            context.mwa_latitude_radians.to_degrees(),
            -26.703_319_405_555_554,
            F64Margin::default()
        ));
        // MWA longitude:            116.67081523611111 degrees
        assert!(approx_eq!(
            f64,
            context.mwa_longitude_radians.to_degrees(),
            116.670_815_236_111_11,
            F64Margin::default()
        ));
        // MWA altitude:             377.827 m,
        assert!(approx_eq!(
            f64,
            context.mwa_altitude_metres,
            377.827,
            F64Margin::default()
        ));

        // obsid:                    1101503312,
        assert_eq!(context.obsid, 1_101_503_312);

        // Creator:                  Randall,
        assert_eq!(context.creator, "Randall");

        // Project ID:               G0009,
        assert_eq!(context.project_id, "G0009");

        // Observation Name:         FDS_DEC-26.7_121,
        assert_eq!(context.observation_name, "FDS_DEC-26.7_121");

        // Receivers:                [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        assert_eq!(context.receivers.len(), 16);
        assert_eq!(context.receivers[0], 1);
        assert_eq!(context.receivers[15], 16);

        // Delays:                   [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        assert_eq!(context.delays.len(), 16);
        assert_eq!(context.delays[0], 0);
        assert_eq!(context.delays[15], 0);

        // Global attenuation:       1 dB,
        assert_eq!(context.global_analogue_attenuation_db as i16, 1);

        // Scheduled start (utc)     2014-12-01 21:08:16 +00:00,
        assert_eq!(
            context.scheduled_start_utc,
            DateTime::parse_from_rfc3339("2014-12-01T21:08:16+00:00").unwrap()
        );

        // Scheduled start (MJD)     56992.88074074074,
        assert!(approx_eq!(
            f64,
            context.scheduled_start_mjd,
            56_992.880_740_740_74,
            F64Margin::default()
        ));

        // Scheduled duration        112 s,
        assert_eq!(context.scheduled_duration_milliseconds, 112_000);

        // Quack time:               2 s,
        assert_eq!(context.quack_time_duration_milliseconds, 2000);

        // Good UNIX start time:     1417468098,
        assert_eq!(context.good_time_unix_milliseconds, 1_417_468_098_000);

        // R.A. (tile_pointing):     144.2107504850443 degrees,
        assert!(approx_eq!(
            f64,
            context.ra_tile_pointing_degrees,
            144.210_750_485_044_3,
            F64Margin::default()
        ));

        // Dec. (tile_pointing):     -26.63403125476213 degrees,
        assert!(approx_eq!(
            f64,
            context.dec_tile_pointing_degrees,
            -26.634_031_254_762_13,
            F64Margin::default()
        ));

        // R.A. (phase center):      None degrees,
        assert!(context.ra_phase_center_degrees.is_none());

        // Dec. (phase center):      None degrees,
        assert!(context.dec_phase_center_degrees.is_none());

        // Azimuth:                  0 degrees,
        assert!(approx_eq!(
            f64,
            context.azimuth_degrees,
            0.,
            F64Margin::default()
        ));

        // Altitude:                 90 degrees,
        assert!(approx_eq!(
            f64,
            context.altitude_degrees,
            90.,
            F64Margin::default()
        ));

        // Sun altitude:             -1.53222775573148 degrees,
        assert!(approx_eq!(
            f64,
            context.sun_altitude_degrees,
            -1.532_227_755_731_48,
            F64Margin::default()
        ));

        // Sun distance:             91.5322277557315 degrees,
        assert!(approx_eq!(
            f64,
            context.sun_distance_degrees,
            91.532_227_755_731_5,
            F64Margin::default()
        ));

        // Moon distance:            131.880015235607 degrees,
        assert!(approx_eq!(
            f64,
            context.moon_distance_degrees,
            131.880_015_235_607,
            F64Margin::default()
        ));

        // Jupiter distance:         41.401684338269 degrees,
        assert!(approx_eq!(
            f64,
            context.jupiter_distance_degrees,
            41.401_684_338_269,
            F64Margin::default()
        ));

        // LST:                      144.381251875516 degrees,
        assert!(approx_eq!(
            f64,
            context.lst_degrees,
            144.381_251_875_516,
            F64Margin::default()
        ));

        // Hour angle:               -00:00:00.00 degrees,
        // Grid name:                sweet,
        assert_eq!(context.grid_name, "sweet");

        // Grid number:              0,
        assert_eq!(context.grid_number, 0);

        // num antennas:             128,
        assert_eq!(context.num_antennas, 128);

        // antennas:                 [Tile011, Tile012, ... Tile167, Tile168],
        assert_eq!(context.antennas[0].tile_name, "Tile011");
        assert_eq!(context.antennas[127].tile_name, "Tile168");

        // rf_inputs:                [Tile011X, Tile011Y, ... Tile168X, Tile168Y],
        assert_eq!(context.num_rf_inputs, 256);
        assert_eq!(context.rf_inputs[0].pol, Pol::X);
        assert_eq!(context.rf_inputs[0].tile_name, "Tile011");
        assert_eq!(context.rf_inputs[255].pol, Pol::Y);
        assert_eq!(context.rf_inputs[255].tile_name, "Tile168");

        // num antenna pols:         2,
        assert_eq!(context.num_antenna_pols, 2);

        // Mode:                     HW_LFILES,
        assert_eq!(context.mode, "HW_LFILES");

        // metafits_filename
        assert_eq!(context.metafits_filename, metafits_filename);
    }
}
