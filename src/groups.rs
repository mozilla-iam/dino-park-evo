use chrono::SecondsFormat;
use chrono::Utc;
use cis_profile::crypto::Signer;
use cis_profile::schema::KeyValue;
use cis_profile::schema::Profile;
use cis_profile::schema::PublisherAuthority::Mozilliansorg;
use failure::Error;
use std::collections::BTreeMap;

pub fn update_groups(
    profile: Profile,
    groups: Vec<String>,
    signer: &impl Signer,
) -> Result<Profile, Error> {
    let mut updated_profile = Profile::default();
    let now = &Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    updated_profile.user_id = profile.user_id;
    updated_profile.active = profile.active;

    let mozillians_groups = groups
        .into_iter()
        .map(|group| (group, None))
        .collect::<BTreeMap<String, Option<String>>>();
    updated_profile.access_information.mozilliansorg = profile.access_information.mozilliansorg;
    if updated_profile
        .access_information
        .mozilliansorg
        .values
        .is_none()
        || updated_profile
            .access_information
            .mozilliansorg
            .metadata
            .created
            .is_empty()
    {
        updated_profile
            .access_information
            .mozilliansorg
            .metadata
            .created = now.to_string();
    }
    updated_profile.access_information.mozilliansorg.values = Some(KeyValue(mozillians_groups));
    updated_profile
        .access_information
        .mozilliansorg
        .signature
        .publisher
        .name = Mozilliansorg;
    updated_profile
        .access_information
        .mozilliansorg
        .metadata
        .last_modified = now.to_string();
    updated_profile
        .access_information
        .mozilliansorg
        .metadata
        .verified = true;

    signer.sign_attribute(&mut updated_profile.access_information.mozilliansorg)?;

    Ok(updated_profile)
}

#[cfg(test)]
mod test {
    use super::*;
    use cis_profile::schema::WithPublisher;

    struct FakeSigner {}
    impl Signer for FakeSigner {
        fn sign_attribute(&self, _: &mut impl WithPublisher) -> Result<(), Error> {
            Ok(())
        }
    }

    #[test]
    fn valid_update() -> Result<(), Error> {
        let profile = Profile::default();
        let updated_profile = update_groups(profile.clone(), vec![], &FakeSigner {})?;
        assert!(updated_profile
            .access_information
            .mozilliansorg
            .values
            .is_some());
        assert_eq!(
            updated_profile
                .access_information
                .mozilliansorg
                .signature
                .publisher
                .name,
            Mozilliansorg
        );
        assert!(
            updated_profile
                .access_information
                .mozilliansorg
                .metadata
                .verified
        );
        assert!(updated_profile
            .access_information
            .mozilliansorg
            .values
            .map(|v| v.0.is_empty())
            .unwrap_or_default());
        Ok(())
    }

    #[test]
    fn groups_get_added() -> Result<(), Error> {
        let profile = Profile::default();
        let updated_profile = update_groups(
            profile.clone(),
            vec![String::from("test1"), String::from("test2")],
            &FakeSigner {},
        )?;
        assert!(updated_profile
            .access_information
            .mozilliansorg
            .values
            .is_some());
        assert!(updated_profile
            .access_information
            .mozilliansorg
            .values
            .map(|v| v.0.keys().len() == 2)
            .unwrap_or_default());
        Ok(())
    }
}
