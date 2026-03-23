#include <stdio.h>
#include <stdint.h>

int main(void) {
    int nblocks = 50000;
    uint64_t mod32 = 4294967296ULL;

    uint64_t pow2[32];
    pow2[0] = 1;
    for (int i = 1; i < 32; i++) pow2[i] = pow2[i-1] * 2;

    uint64_t state[16] = {
        1634760805, 857760878, 2036477234, 1797285236,
        66051, 67438087, 134810123, 202182159,
        269554195, 336926231, 404298267, 471670303,
        0, 0, 100663296, 1207959552
    };

    uint64_t working[16];
    uint64_t checksum = 0;

    for (int block = 0; block < nblocks; block++) {
        state[12] = (uint64_t)block;

        for (int i = 0; i < 16; i++) working[i] = state[i];

        for (int round = 0; round < 10; round++) {
            // Column rounds with full bit-by-bit XOR for first 4 columns
            // Column 1: 0,4,8,12
            working[0] = (working[0] + working[4]) % mod32;
            { uint64_t xa=working[12], xb=working[0], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[12]=xor_acc; }
            working[12] = (working[12]*pow2[16]+working[12]/pow2[16])%mod32;

            working[8] = (working[8]+working[12])%mod32;
            { uint64_t xa=working[4], xb=working[8], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[4]=xor_acc; }
            working[4] = (working[4]*pow2[12]+working[4]/pow2[20])%mod32;

            working[0] = (working[0]+working[4])%mod32;
            { uint64_t xa=working[12], xb=working[0], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[12]=xor_acc; }
            working[12] = (working[12]*pow2[8]+working[12]/pow2[24])%mod32;

            working[8] = (working[8]+working[12])%mod32;
            { uint64_t xa=working[4], xb=working[8], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[4]=xor_acc; }
            working[4] = (working[4]*pow2[7]+working[4]/pow2[25])%mod32;

            // Column 2: 1,5,9,13
            working[1] = (working[1]+working[5])%mod32;
            { uint64_t xa=working[13], xb=working[1], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[13]=xor_acc; }
            working[13] = (working[13]*pow2[16]+working[13]/pow2[16])%mod32;

            working[9] = (working[9]+working[13])%mod32;
            { uint64_t xa=working[5], xb=working[9], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[5]=xor_acc; }
            working[5] = (working[5]*pow2[12]+working[5]/pow2[20])%mod32;

            working[1] = (working[1]+working[5])%mod32;
            { uint64_t xa=working[13], xb=working[1], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[13]=xor_acc; }
            working[13] = (working[13]*pow2[8]+working[13]/pow2[24])%mod32;

            working[9] = (working[9]+working[13])%mod32;
            { uint64_t xa=working[5], xb=working[9], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[5]=xor_acc; }
            working[5] = (working[5]*pow2[7]+working[5]/pow2[25])%mod32;

            // Column 3: 2,6,10,14
            working[2] = (working[2]+working[6])%mod32;
            { uint64_t xa=working[14], xb=working[2], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[14]=xor_acc; }
            working[14] = (working[14]*pow2[16]+working[14]/pow2[16])%mod32;

            working[10] = (working[10]+working[14])%mod32;
            { uint64_t xa=working[6], xb=working[10], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[6]=xor_acc; }
            working[6] = (working[6]*pow2[12]+working[6]/pow2[20])%mod32;

            working[2] = (working[2]+working[6])%mod32;
            { uint64_t xa=working[14], xb=working[2], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[14]=xor_acc; }
            working[14] = (working[14]*pow2[8]+working[14]/pow2[24])%mod32;

            working[10] = (working[10]+working[14])%mod32;
            { uint64_t xa=working[6], xb=working[10], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[6]=xor_acc; }
            working[6] = (working[6]*pow2[7]+working[6]/pow2[25])%mod32;

            // Column 4: 3,7,11,15
            working[3] = (working[3]+working[7])%mod32;
            { uint64_t xa=working[15], xb=working[3], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[15]=xor_acc; }
            working[15] = (working[15]*pow2[16]+working[15]/pow2[16])%mod32;

            working[11] = (working[11]+working[15])%mod32;
            { uint64_t xa=working[7], xb=working[11], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[7]=xor_acc; }
            working[7] = (working[7]*pow2[12]+working[7]/pow2[20])%mod32;

            working[3] = (working[3]+working[7])%mod32;
            { uint64_t xa=working[15], xb=working[3], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[15]=xor_acc; }
            working[15] = (working[15]*pow2[8]+working[15]/pow2[24])%mod32;

            working[11] = (working[11]+working[15])%mod32;
            { uint64_t xa=working[7], xb=working[11], xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[7]=xor_acc; }
            working[7] = (working[7]*pow2[7]+working[7]/pow2[25])%mod32;

            // Diagonal rounds (simplified mixing)
            // Diagonal 1: 0,5,10,15
            working[0]=(working[0]+working[5])%mod32;
            { uint64_t xa=working[15],xb=working[0],xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[15]=xor_acc;}
            working[15]=(working[15]*pow2[16]+working[15]/pow2[16])%mod32;
            working[10]=(working[10]+working[15])%mod32;
            { uint64_t xa=working[5],xb=working[10],xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[5]=xor_acc;}
            working[5]=(working[5]*pow2[12]+working[5]/pow2[20])%mod32;
            working[0]=(working[0]+working[5])%mod32;
            { uint64_t xa=working[15],xb=working[0],xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[15]=xor_acc;}
            working[15]=(working[15]*pow2[8]+working[15]/pow2[24])%mod32;
            working[10]=(working[10]+working[15])%mod32;
            { uint64_t xa=working[5],xb=working[10],xor_acc=0;
              for(int bit=0;bit<32;bit++){uint64_t ba=(xa/pow2[bit])%2,bb=(xb/pow2[bit])%2;xor_acc+=(ba+bb-2*ba*bb)*pow2[bit];}
              working[5]=xor_acc;}
            working[5]=(working[5]*pow2[7]+working[5]/pow2[25])%mod32;

            // Diagonal 2: 1,6,11,12
            working[1]=(working[1]+working[6])%mod32;
            working[12]=((working[12]+working[1])*pow2[16]%mod32+(working[12]+working[1])/pow2[16])%mod32;
            working[11]=(working[11]+working[12])%mod32;
            working[6]=((working[6]+working[11])*pow2[12]%mod32+(working[6]+working[11])/pow2[20])%mod32;
            working[1]=(working[1]+working[6])%mod32;
            working[12]=((working[12]+working[1])*pow2[8]%mod32+(working[12]+working[1])/pow2[24])%mod32;
            working[11]=(working[11]+working[12])%mod32;
            working[6]=((working[6]+working[11])*pow2[7]%mod32+(working[6]+working[11])/pow2[25])%mod32;

            // Diagonal 3: 2,7,8,13
            working[2]=(working[2]+working[7])%mod32;
            working[13]=((working[13]+working[2])*pow2[16]%mod32+(working[13]+working[2])/pow2[16])%mod32;
            working[8]=(working[8]+working[13])%mod32;
            working[7]=((working[7]+working[8])*pow2[12]%mod32+(working[7]+working[8])/pow2[20])%mod32;
            working[2]=(working[2]+working[7])%mod32;
            working[13]=((working[13]+working[2])*pow2[8]%mod32+(working[13]+working[2])/pow2[24])%mod32;
            working[8]=(working[8]+working[13])%mod32;
            working[7]=((working[7]+working[8])*pow2[7]%mod32+(working[7]+working[8])/pow2[25])%mod32;

            // Diagonal 4: 3,4,9,14
            working[3]=(working[3]+working[4])%mod32;
            working[14]=((working[14]+working[3])*pow2[16]%mod32+(working[14]+working[3])/pow2[16])%mod32;
            working[9]=(working[9]+working[14])%mod32;
            working[4]=((working[4]+working[9])*pow2[12]%mod32+(working[4]+working[9])/pow2[20])%mod32;
            working[3]=(working[3]+working[4])%mod32;
            working[14]=((working[14]+working[3])*pow2[8]%mod32+(working[14]+working[3])/pow2[24])%mod32;
            working[9]=(working[9]+working[14])%mod32;
            working[4]=((working[4]+working[9])*pow2[7]%mod32+(working[4]+working[9])/pow2[25])%mod32;
        }

        for (int i = 0; i < 16; i++) working[i] = (working[i]+state[i])%mod32;
        for (int i = 0; i < 16; i++) checksum = (checksum+working[i])%mod32;
    }

    printf("%lld\n", (long long)checksum);
    return 0;
}
